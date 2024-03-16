use std::{path::PathBuf, str::FromStr};

use nom::{
    bytes::complete::{escaped, tag, take, take_till, take_till1},
    character::complete::{alpha1, char, newline, space0, space1},
    combinator::recognize,
    sequence::{delimited, preceded, Tuple},
    Err, Finish, IResult,
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

type Result<'a, O = Input<'a>, I = Input<'a>, E = ParseError<I>> = IResult<I, O, E>;

impl<I> nom::error::ParseError<I> for ParseError<I> {
    fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
        ParseError::Nom(input, kind)
    }

    fn append(input: I, kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

fn string(input: Input) -> Result<String> {
    let (rest, raw_string) = recognize(delimited(
        char('"'),
        escaped(take_till1(|x| x == '"' || x == '\\'), '\\', take(1usize)),
        char('"'),
    ))(input)?;

    let parsed_string: String = serde_json::from_str(raw_string.as_ref())
        .or(Err(Err::Failure(ParseError::MalformedString(raw_string))))?;

    Ok((rest, parsed_string))
}

fn line_comment(input: Input) -> Result {
    recognize(preceded(char(';'), take_till(|x| x == '\n')))(input)
}

#[derive(Debug)]
enum Directive {
    Include(RelativePathBuf),
}

fn include_directive(input: Input) -> Result<Directive> {
    let (rest, (_, _, path, _)) = (tag("#include"), space1, string, space0).parse(input)?;

    let path =
        PathBuf::from_str(path.as_str()).or(Err(Err::Failure(ParseError::MalformedPath(input))))?;
    let path = RelativePathBuf::from_path(path)
        .or(Err(Err::Failure(ParseError::NotRelativePath(input))))?;

    Ok((rest, Directive::Include(path)))
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use super::*;

    fn ok<'a, O, E>(result: Result<'a, O, Input, E>)
    where
        O: Debug,
        E: Debug,
    {
        let result = result.finish();
        assert!(result.is_ok(), "{:?}", result);

        let result = result.unwrap();
        assert_eq!(
            result.0.len(),
            0,
            "not all of input was consumed, {:?}",
            result
        );
    }

    fn bad<'a, O, E>(result: Result<'a, O, Input, E>)
    where
        O: Debug,
        E: Debug,
    {
        let result = result.finish();
        assert!(result.is_err(), "{:?}", result);
    }

    #[test]
    fn parse_string() {
        ok(string(r#""this is a string!""#.into()));
        ok(string(r#""I will say \"Hello, world!\", ok?""#.into()));
        bad(string(
            r#""I'm gonna asd\a\sd\a\\asdasad\\as\d\as\d\a""#.into(),
        ));
        ok(string(r#""I'm gonna \\n fake new line""#.into()));
        bad(string(r#""I'm gonna \""#.into()));

        ok(line_comment(r#"; this is a comment, ashuidasj"#.into()));
        ok(line_comment(r#"; this is a comment, ashuidasj  "#.into()));

        ok(include_directive(
            r#"#include "./test/tcp.rtconfig.txt""#.into(),
        ));
        ok(include_directive(
            r#"#include    "./test/tcp.rtconfig.txt"  "#.into(),
        ));
        bad(include_directive(
            r#"#include    "C:/test/tcp.rtconfig.txt"  "#.into(),
        ));
    }
}
