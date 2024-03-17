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
    interpreter::{self, Color},
    parser::{self, parse_reapertheme, Directive, ParseError, ReaperThemeContent, RtconfigContent},
    theme::ResourceMap,
};

#[derive(Error, Debug)]
pub enum PreprocessError {
    #[error("cannot include a file outside the root folder `{0}`")]
    IncludeOutsideRoot(PathBuf),
    #[error("cannot add a resource outside the root folder `{0}`")]
    ResourceOutsideRoot(PathBuf),
    #[error("failed to read file `{0}`")]
    ReadError(PathBuf),
    #[error("failed to parse rtconfig `{0}`")]
    RtconfigParseError(PathBuf, ParseError),
    #[error("failed to parse reapertheme `{0}`")]
    ReaperThemeParseError(PathBuf, ParseError),
    #[error("failed to read reapertheme file `{0}`")]
    IniError(#[from] ini::Error),
    #[error("failed to read script file `{0}`")]
    ReadScriptError(std::io::Error),
    #[error("failed to evaluate lua code `{0}`")]
    EvaluateError(#[from] mlua::Error),
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

struct ThemeBuilder<'a> {
    root: PathBuf,
    lua: mlua::Lua,
    parts: Vec<Cow<'a, str>>,
    config: Ini,
    resources: ResourceMap,
}

impl<'a> ThemeBuilder<'a> {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            lua: interpreter::new(),
            parts: Vec::new(),
            config: Ini::new(),
            resources: HashMap::new(),
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

    fn feed(&mut self, content: RtconfigContent<'a>, source_path: &Path) -> Result {
        match content {
            RtconfigContent::Newline => self.parts.push("\n".into()),
            RtconfigContent::Code(text) => self.parts.push(Cow::Borrowed(text.fragment())),
            RtconfigContent::Comment(text) => self.parts.push(Cow::Borrowed(text.fragment())),
            RtconfigContent::Expression(text) => self.feed_expression(text)?,
            RtconfigContent::Directive(dir) => match dir {
                Directive::Include(path) => self.feed_directive_include(&path),
                Directive::Resource { pattern, dest } => {
                    self.feed_directive_resource(&pattern, &dest, &source_path)
                }
                Directive::Unknown { name, contents } => {
                    self.feed_directive_unknown(name, contents)
                }
            },
        };
        Ok(())
    }

    fn import_config(&mut self, path: &Path) -> Result {
        let ini = Ini::load_from_file(path)?;

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
                            self.serialise_expression(*text, false)
                        }
                    })
                    .collect();
                let value = value?;

                self.config.with_section(section).set(key, value);
            }
        }

        Ok(())
    }

    fn run_script(&mut self, path: &Path) -> Result {
        let script =
            std::fs::read_to_string(path).map_err(|err| PreprocessError::ReadScriptError(err))?;
        self.lua
            .load(script)
            .set_name(path.to_string_lossy())
            .exec()?;
        Ok(())
    }

    fn serialise_expression(&self, expr: parser::Input, is_rtconfig: bool) -> Result<Cow<str>> {
        let value: mlua::Value = self
            .lua
            .load(*expr.fragment())
            .set_mode(mlua::ChunkMode::Text)
            .set_name(format!(
                "Line {} Column {} `{}`",
                expr.location_line(),
                expr.get_utf8_column(),
                expr.fragment()
            ))
            .eval()?;

        match value {
            mlua::Value::Nil => Ok("".into()),
            mlua::Value::Boolean(true) => Ok("true".into()),
            mlua::Value::Boolean(false) => Ok("false".into()),
            mlua::Value::Integer(x) => Ok(x.to_string().into()),
            mlua::Value::Number(x) => Ok(x.to_string().into()),
            mlua::Value::String(x) => Ok(x
                .to_str()
                .expect("expression evaluated into invalid utf8 string")
                .to_string()
                .into()),
            mlua::Value::Table(_) => todo!(),
            mlua::Value::Function(_) => todo!(),
            mlua::Value::Thread(_) => todo!(),
            mlua::Value::UserData(userdata) => {
                if let Ok(color) = userdata.take::<Color>() {
                    if is_rtconfig {
                        Ok(color.value().to_string().into())
                    } else {
                        Ok(color.value_rev().to_string().into())
                    }
                } else {
                    todo!()
                }
            }
            mlua::Value::LightUserData(_) => todo!(),
            mlua::Value::Error(_) => todo!(),
        }
    }

    fn feed_expression(&mut self, expr: parser::Input) -> Result {
        let expr = self.serialise_expression(expr, true)?;
        let expr = expr.to_string();

        self.parts.push(expr.into());

        Ok(())
    }

    fn feed_directive_include(&mut self, path: &RelativePath) {
        todo!()
    }

    fn feed_directive_resource(
        &mut self,
        pattern: &Pattern,
        dest: &RelativePath,
        source_path: &Path,
    ) {
        debug!(
            "glob pattern `{}` starting from `{}`",
            pattern,
            source_path.to_string_lossy()
        );

        let absolute_pattern = source_path.join(pattern.as_str());
        let resources = glob::glob(absolute_pattern.to_string_lossy().as_ref()).expect(
            format!(
                "invalid glob pattern `{}`",
                absolute_pattern.to_string_lossy()
            )
            .as_str(),
        );

        for path in resources {
            debug!("{path:?}");
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

    fn feed_directive_unknown(&mut self, name: parser::Input, contents: parser::Input) {
        todo!()
    }
}

pub fn preprocess(path: &Path, working_directory: Option<&Path>) -> Result<ResourceMap> {
    let text = read(&path)?;
    let contents = parse_rtconfig(&path, &text)?;
    let lua = interpreter::new();
    let working_directory = working_directory
        .map(|p| p.to_path_buf())
        .unwrap_or(std::env::current_dir().unwrap());
    let mut resources: ResourceMap = HashMap::new();
    let mut result: Vec<String> = vec![];

    // let processed_contents: Vec<_> = contents
    //     .iter()
    //     .map(|content| match content {
    //         RtconfigContent::Expression(expr) => todo!(),
    //         RtconfigContent::Directive(dir) => match dir {
    //             Directive::Include(_) => todo!(),
    //             Directive::Resource { pattern, dest } => {
    //                 add_resources(&mut resources, &pattern, &dest, &working_directory)
    //             }
    //             Directive::Unknown { name, contents } => todo!(),
    //         },
    //         x => x,
    //     })
    //     .collect();

    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn feed<'a>(builder: &mut ThemeBuilder<'a>, content: RtconfigContent<'a>) {
        builder.feed(content, ".".as_ref()).unwrap();
    }

    #[test]
    fn test_01() {
        let mut builder = ThemeBuilder::new(".".as_ref());

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
}
