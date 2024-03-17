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
    interpreter,
    parser::{self, Directive, ParseError, RtconfigContent},
    theme::ResourceMap,
};

#[derive(Error, Debug)]
pub enum PreprocessError {
    #[error("failed to read file `{0}`")]
    ReadError(PathBuf),
    #[error("failed to parse rtconfig `{0}`")]
    ParseError(PathBuf, ParseError),
}

impl From<mlua::Error> for PreprocessError {
    fn from(value: mlua::Error) -> Self {
        dbg!(&value);
        todo!()
    }
}

type Result<I = ()> = std::result::Result<I, PreprocessError>;

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).or(Err(PreprocessError::ReadError(path.to_path_buf())))
}

fn parse<'text, 'path>(path: &'path Path, text: &'text str) -> Result<Vec<RtconfigContent<'text>>> {
    parser::parse(&text).map_err(|err| PreprocessError::ParseError(path.to_path_buf(), err))
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

    fn feed(&mut self, content: RtconfigContent<'a>, source_path: PathBuf) -> Result {
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

    fn feed_expression(&mut self, expr: parser::Input) -> Result {
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
    let contents = parse(&path, &text)?;
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
