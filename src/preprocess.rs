use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    parser::{self, ParseError, RtconfigContent},
    theme::ResourceMap,
};

#[derive(Error, Debug)]
pub enum PreprocessError {
    #[error("failed to read file `{0}`")]
    ReadError(PathBuf),
    #[error("failed to parse rtconfig `{0}`")]
    ParseError(PathBuf, ParseError),
}

type Result<I = ()> = std::result::Result<I, PreprocessError>;

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).or(Err(PreprocessError::ReadError(path.to_path_buf())))
}

fn parse<'text, 'path>(path: &'path Path, text: &'text str) -> Result<Vec<RtconfigContent<'text>>> {
    parser::parse(&text).map_err(|err| PreprocessError::ParseError(path.to_path_buf(), err))
}

pub fn preprocess(path: &Path, working_directory: Option<&Path>) -> Result<ResourceMap> {
    let text = read(&path)?;
    let contents = parse(&path, &text)?;

    let processed_contents: Vec<_> = contents.iter().map(|content| {
        match content {
            RtconfigContent::Expression(expr) => todo!(),
            RtconfigContent::Directive(dir) => match dir {
                parser::Directive::Include(_) => todo!(),
                parser::Directive::Resource { pattern, dest } => todo!(),
                parser::Directive::Unknown { name, contents } => todo!(),
            },
            x => x,
        }
    }).collect();

    todo!()
}
