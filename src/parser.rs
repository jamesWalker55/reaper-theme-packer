use std::{path::PathBuf, str::FromStr};

use nom::{
    bytes::complete::{escaped, tag, take, take_till, take_till1},
    character::complete::{alpha1, char, newline, space0, space1},
    combinator::recognize,
    sequence::{delimited, preceded, Tuple},
    Err, IResult,
};
use nom_locate::LocatedSpan;
use relative_path::RelativePathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
enum ParseError<I> {
    #[error("invalid string literal `{0}`")]
    MalformedString(I),
    #[error("invalid path `{0}`")]
    MalformedPath(I),
    #[error("only relative paths are allowed `{0}`")]
    NotRelativePath(I),
    #[error("invalid syntax: {0:?}")]
    Nom(I, nom::error::ErrorKind),
}

type Input<'a> = LocatedSpan<&'a str>;

impl<I> nom::error::ParseError<I> for ParseError<I> {
    fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
        ParseError::Nom(input, kind)
    }

    fn append(input: I, kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

fn string(input: Input) -> IResult<Input, String, ParseError<Input>> {
    let (rest, raw_string) = recognize(delimited(
        char('"'),
        escaped(take_till1(|x| x == '"' || x == '\\'), '\\', take(1usize)),
        char('"'),
    ))(input)?;

    let parsed_string: String = serde_json::from_str(raw_string.as_ref())
        .or(Err(Err::Failure(ParseError::MalformedString(raw_string))))?;

    Ok((rest, parsed_string))
}

fn line_comment(input: Input) -> IResult<Input, Input, ParseError<Input>> {
    recognize(preceded(char(';'), take_till(|x| x == '\n')))(input)
}

#[derive(Debug)]
enum Directive {
    Include(RelativePathBuf),
}

fn include_directive(input: Input) -> IResult<Input, Directive, ParseError<Input>> {
    let (rest, (_, _, path, _)) = (tag("#include"), space1, string, space0).parse(input)?;

    let path =
        PathBuf::from_str(path.as_str()).or(Err(Err::Failure(ParseError::MalformedPath(input))))?;
    let path = RelativePathBuf::from_path(path)
        .or(Err(Err::Failure(ParseError::NotRelativePath(input))))?;

    Ok((rest, Directive::Include(path)))
}

#[test]
fn parse_string() {
    dbg!(string(r#""this is a string! I will say \"Hello, world!\", ok?""#.into()).unwrap());
    dbg!(line_comment(r#"; this is a comment, ashuidasj"#.into()).unwrap());
    dbg!(include_directive(r#"#include "./test/tcp.rtconfig.txt""#.into()).unwrap());
    dbg!(include_directive(r#"#include    "./test/tcp.rtconfig.txt"  "#.into()).unwrap());
}
