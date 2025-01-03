use std::{
    borrow::Cow,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use glob::Pattern;
use ini::Ini;
use log::{debug, warn};
use relative_path::RelativePath;

use thiserror::Error;

use crate::{
    interpreter::{self, RGB, RGBA},
    parser::{
        self, parse_reapertheme, Directive, ErrorLocation, ParseError, ReaperThemeContent,
        RtconfigContent,
    },
    theme::ResourceMap,
};

#[derive(Error, Debug)]
pub enum PreprocessError {
    #[error("{0}: cannot include a file outside the root folder")]
    IncludeOutsideRoot(PathBuf),
    #[error("{0}: cannot add a resource outside the root folder")]
    ResourceOutsideRoot(PathBuf),
    #[error("{0}: failed to read file")]
    ReadError(PathBuf),
    #[error("{0}:{}: {1}", .1.location())]
    RtconfigParseError(PathBuf, ParseError),
    #[error("{0}:{}: {1}", .1.location())]
    ReaperThemeParseError(PathBuf, ParseError),
    #[error("{0}: failed to read reapertheme file {1}")]
    IniError(PathBuf, ini::Error),
    #[error("{0}: failed to read script file ({1})")]
    ReadScriptError(PathBuf, std::io::Error),
    #[error("{0}:{1}: failed to evaluate lua code: {2}")]
    EvaluateError(PathBuf, ErrorLocation, mlua::Error),
}

impl PreprocessError {
    fn path(&self) -> &Path {
        match self {
            PreprocessError::IncludeOutsideRoot(path) => path.as_path(),
            PreprocessError::ResourceOutsideRoot(path) => path.as_path(),
            PreprocessError::ReadError(path) => path.as_path(),
            PreprocessError::RtconfigParseError(path, _) => path.as_path(),
            PreprocessError::ReaperThemeParseError(path, _) => path.as_path(),
            PreprocessError::IniError(path, _) => path.as_path(),
            PreprocessError::ReadScriptError(path, _) => path.as_path(),
            PreprocessError::EvaluateError(path, _, _) => path.as_path(),
        }
    }

    fn message(&self) -> &'static str {
        match self {
            Self::IncludeOutsideRoot(..) => "cannot include a file outside the root folder",
            Self::ResourceOutsideRoot(..) => "cannot add a resource outside the root folder",
            Self::ReadError(..) => "failed to read file",
            Self::RtconfigParseError(..) => "failed to parse rtconfig",
            Self::ReaperThemeParseError(..) => "failed to parse reapertheme",
            Self::IniError(..) => "failed to read reapertheme file",
            Self::ReadScriptError(..) => "failed to read script file",
            Self::EvaluateError(..) => "failed to evaluate lua code",
        }
    }
}

type Result<I = ()> = std::result::Result<I, PreprocessError>;

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).or(Err(PreprocessError::ReadError(path.to_path_buf())))
}

fn parse_rtconfig<'text, 'path>(
    path: &'path Path,
    text: &'text str,
) -> Result<Vec<RtconfigContent<'text>>> {
    parser::parse_rtconfig(&text)
        .map_err(|err| PreprocessError::RtconfigParseError(path.to_path_buf(), err))
}

enum IncludeType {
    RtConfig,
    ReaperTheme,
    Lua,
}

struct ThemeBuilder {
    lua: mlua::Lua,
    parts: Vec<String>,
    config: Ini,
    resources: ResourceMap,
    skip_next_newline: bool,
}

impl ThemeBuilder {
    fn new() -> Self {
        Self {
            lua: interpreter::new(),
            parts: Vec::new(),
            config: Ini::new(),
            resources: HashMap::new(),
            skip_next_newline: false,
        }
    }

    fn rtconfig(&self) -> String {
        self.parts.join("")
    }

    fn reapertheme(&self) -> &Ini {
        &self.config
    }

    fn resources(&self) -> &ResourceMap {
        &self.resources
    }

    fn feed(&mut self, content: &RtconfigContent, source_path: &Path) -> Result {
        match content {
            RtconfigContent::Newline => {
                if self.skip_next_newline {
                    self.skip_next_newline = false;
                } else {
                    self.parts.push("\n".into());
                }
            }
            RtconfigContent::Code(text) => self.parts.push(text.fragment().to_string()),
            RtconfigContent::Comment(text) => self.parts.push(text.fragment().to_string()),
            RtconfigContent::Expression(text) => self.feed_expression(text).and_then(|_| {
                let mut directives = interpreter::NEW_RESOURCE_PATHS.lock().unwrap();
                for x in directives.iter() {
                    let Directive::Resource { pattern, dest } = x else {
                        panic!("NEW_RESOURCE_PATHS should only contain Directive::Resource instances")
                    };
                    self.feed_directive_resource(&pattern, &dest, &source_path);
                }
                directives.clear();;
                Ok(())
            }).map_err(|err| {
                PreprocessError::EvaluateError(source_path.into(), text.into(), err)
            })?,
            RtconfigContent::Directive(dir) => {
                self.skip_next_newline = true;
                match dir {
                    Directive::Include(path) => self.feed_directive_include(&path, &source_path)?,
                    Directive::Resource { pattern, dest } => {
                        self.feed_directive_resource(&pattern, &dest, &source_path)
                    }
                    Directive::Unknown { name, contents } => {
                        self.feed_directive_unknown(name, contents)
                    }
                }
            }
        };
        Ok(())
    }

    fn import_config(&mut self, path: &Path) -> Result {
        let ini = Ini::load_from_file(path)
            .map_err(|err| PreprocessError::IniError(path.to_path_buf(), err))?;

        for (section, prop) in ini.iter() {
            for (key, value) in prop.iter() {
                // parse the value to find expressions
                let value = parse_reapertheme(value).map_err(|err| {
                    PreprocessError::ReaperThemeParseError(path.to_path_buf(), err)
                })?;

                // evaluate any expressions and join to string
                let value: Result<String> = value
                    .iter()
                    .map(|x| match x {
                        ReaperThemeContent::Text(text) => Ok(Cow::from(*text.fragment())),
                        ReaperThemeContent::Expression(text) => {
                            self.serialise_expression(text, false).map_err(|err| {
                                PreprocessError::EvaluateError(path.into(), text.into(), err)
                            })
                        }
                    })
                    .collect();
                let value = value?;

                self.config.with_section(section).set(key, value);
            }
        }

        Ok(())
    }

    fn run_script(&self, path: &Path) -> Result {
        let script = std::fs::read_to_string(path)
            .map_err(|err| PreprocessError::ReadScriptError(path.to_path_buf(), err))?;
        self.lua
            .load(script)
            .set_name(path.to_string_lossy())
            .exec()
            .map_err(|err| {
                PreprocessError::EvaluateError(path.into(), ErrorLocation::default(), err)
            })?;
        Ok(())
    }

    fn serialise_expression(
        &self,
        expr: &parser::Input,
        is_rtconfig: bool,
    ) -> mlua::Result<Cow<str>> {
        let value: mlua::Value = self
            .lua
            .load(*expr.fragment())
            .set_mode(mlua::ChunkMode::Text)
            .set_name(*expr.fragment())
            .eval()?;

        match value {
            mlua::Value::Nil => Ok("".into()),
            mlua::Value::Boolean(true) => Ok("true".into()),
            mlua::Value::Boolean(false) => Ok("false".into()),
            mlua::Value::Integer(x) => Ok(x.to_string().into()),
            mlua::Value::Number(x) => Ok(x.to_string().into()),
            mlua::Value::String(x) => {
                let column = expr.get_utf8_column() - 3;
                let x = x
                    .to_str()
                    .expect("expression evaluated into invalid utf8 string")
                    .to_string();
                let indented_x = indent::indent_by(column, x);
                Ok(indented_x.into())
            }
            mlua::Value::Table(_) => todo!("Table"),
            mlua::Value::Function(_) => todo!("Function"),
            mlua::Value::Thread(_) => todo!("Thread"),
            mlua::Value::UserData(userdata) => {
                if let Ok(color) = userdata.borrow::<RGB>() {
                    Ok(color.value_rev().to_string().into())
                } else if let Ok(color) = userdata.borrow::<RGBA>() {
                    Ok(color.value_rev().to_string().into())
                } else {
                    todo!("UserData")
                }
            }
            mlua::Value::LightUserData(_) => todo!("LightUserData"),
            mlua::Value::Other(..) => todo!("Other"),
            mlua::Value::Error(_) => todo!("Error"),
        }
    }

    fn feed_expression(&mut self, expr: &parser::Input) -> mlua::Result<()> {
        let expr = self.serialise_expression(expr, true)?;
        let expr = expr.to_string();

        self.parts.push(expr.into());

        Ok(())
    }

    fn determine_include_type(path: &RelativePath) -> IncludeType {
        match path.extension().map(|x| x.to_ascii_lowercase()) {
            Some(ext) => match ext.as_str() {
                "reapertheme" | "ini" => IncludeType::ReaperTheme,
                "lua" => IncludeType::Lua,
                _ => IncludeType::RtConfig,
            },
            None => IncludeType::RtConfig,
        }
    }

    fn feed_directive_include(
        &mut self,
        include_relpath: &RelativePath,
        source_path: &Path,
    ) -> Result {
        let include_type = Self::determine_include_type(include_relpath);
        let include_path = include_relpath.to_path(source_path.parent().unwrap());

        match include_type {
            IncludeType::RtConfig => panic!("#include rtconfig should not be fed into builder"),
            IncludeType::ReaperTheme => self.import_config(&include_path)?,
            IncludeType::Lua => self.run_script(&include_path)?,
        }

        Ok(())
    }

    fn feed_directive_resource(
        &mut self,
        pattern: &Pattern,
        dest: &RelativePath,
        source_path: &Path,
    ) {
        let source_dir = source_path.parent().unwrap();
        debug!(
            "glob pattern `{}` starting from `{}`",
            pattern,
            source_dir.to_string_lossy()
        );

        let absolute_pattern = source_dir.join(pattern.as_str());
        let resources = glob::glob(absolute_pattern.to_string_lossy().as_ref()).expect(
            format!(
                "invalid glob pattern `{}`",
                absolute_pattern.to_string_lossy()
            )
            .as_str(),
        );

        for path in resources {
            match path {
                Err(err) => warn!(
                    "failed to get resources in path `{}`: {}",
                    err.path().to_string_lossy(),
                    err.error()
                ),
                Ok(path) => match path.file_name() {
                    None => warn!(
                        "resource does not have a filename `{}`",
                        path.to_string_lossy()
                    ),
                    Some(file_name) => {
                        let dest_file = dest.join(file_name.to_string_lossy().as_ref());
                        if self.resources.contains_key(&dest_file) {
                            warn!(
                                "resource `{}` overwrites previous resource at `{}`",
                                path.to_string_lossy(),
                                dest_file
                            );
                            continue;
                        }

                        self.resources.insert(dest_file, path);
                    }
                },
            }
        }
    }

    fn feed_directive_unknown(&mut self, name: &parser::Input, contents: &parser::Input) {
        self.parts.push(format!("; #{name}{contents}\n"));
    }
}

fn _preprocess(mut builder: &mut ThemeBuilder, path: &Path) -> Result {
    let text = read(&path)?;
    let contents = parse_rtconfig(&path, &text)?;

    for content in &contents {
        if let RtconfigContent::Directive(Directive::Include(include_relpath)) = content {
            let include_path = include_relpath.to_path(path.parent().unwrap());
            match ThemeBuilder::determine_include_type(&include_relpath) {
                IncludeType::RtConfig => _preprocess(&mut builder, &include_path)?,
                _ => builder.feed(&content, path)?,
            }
        } else {
            builder.feed(&content, path)?;
        }
    }

    Ok(())
}

pub fn preprocess(
    path: &Path,
    globals: Option<HashMap<String, String>>,
) -> Result<(String, Ini, ResourceMap)> {
    let mut builder = ThemeBuilder::new();

    if let Some(globals) = globals {
        let table = builder.lua.globals();
        for (k, v) in globals {
            table
                .set(k.clone(), v.clone())
                .expect(format!("failed to set global {}={:?}", k, v).as_str());
        }
    };

    _preprocess(&mut builder, &path)?;

    Ok((
        builder.rtconfig(),
        builder.reapertheme().clone(),
        builder.resources().clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn feed(builder: &mut ThemeBuilder, content: RtconfigContent) {
        builder.feed(&content, ".".as_ref()).unwrap();
    }

    #[test]
    fn test_01() {
        let mut builder = ThemeBuilder::new();

        feed(
            &mut builder,
            RtconfigContent::Code("set test [1 2 3 4]".into()),
        );
        feed(&mut builder, RtconfigContent::Newline);
        feed(
            &mut builder,
            RtconfigContent::Code("set test [1 2 3 4]".into()),
        );

        feed(&mut builder, RtconfigContent::Newline);
        feed(&mut builder, RtconfigContent::Expression("1 + 5".into()));
        feed(&mut builder, RtconfigContent::Newline);
        feed(
            &mut builder,
            RtconfigContent::Expression("rgb(1, 2, 3)".into()),
        );
        feed(&mut builder, RtconfigContent::Newline);

        assert_eq!(
            builder.rtconfig(),
            indoc! {"
                set test [1 2 3 4]
                set test [1 2 3 4]
                6
                66051
            "}
        );
    }

    #[test]
    fn test_02() {
        crate::setup_logging();

        match preprocess(r"test\test.rtconfig.txt".as_ref(), None) {
            Ok((rtconfig, reapertheme, res)) => {
                let mut new_res: HashMap<String, String> = HashMap::new();
                for (k, v) in res.iter() {
                    new_res.insert(k.to_string(), v.to_string_lossy().to_string());
                }

                fs::write("out.rtconfig.txt", rtconfig).unwrap();
                reapertheme.write_to_file("out.ReaperTheme").unwrap();
                fs::write(
                    "out.res.json",
                    serde_json::to_string_pretty(&new_res).unwrap(),
                )
                .unwrap();
            }
            Err(err) => println!("[ERROR] {}", err),
        };
    }
}
