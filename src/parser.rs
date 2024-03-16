use std::{path::PathBuf, str::FromStr};

use nom::{
    bytes::complete::{escaped, tag, take, take_till, take_till1},
    character::complete::{alpha1, char, space0, space1},
    combinator::{opt, recognize},
    sequence::{delimited, preceded, terminated, tuple, Tuple},
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
    #[error("invalid glob pattern `{0}`")]
    InvalidGlobPattern(I),
    #[error("unknown directive `{0}`")]
    UnknownDirective(I),
    #[error("invalid syntax: {0:?}")]
    Nom(I, nom::error::ErrorKind),
}

type Input<'a> = LocatedSpan<&'a str>;

type Result<'a, O = Input<'a>> = IResult<Input<'a>, O, ParseError<Input<'a>>>;

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
    Resource {
        pattern: glob::Pattern,
        dest: RelativePathBuf,
    },
}

fn relative_path_string(input: Input) -> Result<RelativePathBuf> {
    let (rest, raw_string) = string(input)?;

    // convert to PathBuf, may be absolute
    let path = PathBuf::from_str(raw_string.as_str())
        .or(Err(Err::Failure(ParseError::MalformedPath(input))))?;
    // convert to RelativePathBuf, must be relative now
    let path = RelativePathBuf::from_path(path)
        .or(Err(Err::Failure(ParseError::NotRelativePath(input))))?;

    Ok((rest, path))
}

fn include_directive(input: Input) -> Result<Directive> {
    let (rest, (_, _, path, _)) =
        (tag("#include"), space1, relative_path_string, space0).parse(input)?;

    Ok((rest, Directive::Include(path)))
}

fn resource_directive(input: Input) -> Result<Directive> {
    let (rest, (_, _, dest, pattern, _)) = (
        tag("#resource"),
        space1,
        opt(terminated(
            relative_path_string,
            tuple((space0, char(':'), space0)),
        )),
        relative_path_string,
        space0,
    )
        .parse(input)?;

    // default destination to "."
    let dest = dest.unwrap_or(RelativePathBuf::from("."));

    // parse pattern
    let pattern = glob::Pattern::new(pattern.as_str())
        .or(Err(Err::Failure(ParseError::InvalidGlobPattern(input))))?;

    Ok((rest, Directive::Resource { pattern, dest }))
}

fn unknown_directive(input: Input) -> Result {
    let (rest, (_, name, contents)) = (char('#'), alpha1, take_till(|x| x == '\n')).parse(input)?;

    Err(Err::Failure(ParseError::UnknownDirective(input)))
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use nom::Finish;

    use super::*;

    fn ok<'a, O>(result: Result<'a, O>)
    where
        O: Debug,
    {
        let result = result.finish();
        dbg!(&result);
        assert!(result.is_ok(), "should be ok: {:?}", result);

        let result = result.unwrap();
        assert_eq!(
            result.0.len(),
            0,
            "not all of input was consumed, {:?}",
            result
        );
    }

    fn bad<'a, O>(result: Result<'a, O>)
    where
        O: Debug,
    {
        let result = result.finish();
        dbg!(&result);
        match result {
            Ok(result) => {
                assert!(result.0.len() > 0, "should be err: {:?}", result);
            }
            Err(_) => (),
        }
    }

    fn irrecoverable<'a, O>(result: Result<'a, O>)
    where
        O: Debug,
    {
        dbg!(&result);
        match result {
            Err(Err::Failure(_)) => (),
            _ => panic!("should be Err::Failure"),
        }
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

        irrecoverable(unknown_directive(
            r#"#include "./test/tcp.rtconfig.txt""#.into(),
        ));
        irrecoverable(unknown_directive(
            r#"#include    "./test/tcp.rtconfig.txt"  "#.into(),
        ));
        irrecoverable(unknown_directive(
            r#"#include    "C:/test/tcp.rtconfig.txt"  "#.into(),
        ));

        ok(resource_directive(r#"#resource "./*.png""#.into()));
        ok(resource_directive(r#"#resource    "./*.png"  "#.into()));
        ok(resource_directive(r#"#resource "./knob.png""#.into()));
        ok(resource_directive(r#"#resource "150": "./*.png""#.into()));
        ok(resource_directive(
            r#"#resource    "150" : "./*.png"    "#.into(),
        ));
        bad(resource_directive(r#"#resource "150" "./*.png""#.into()));
        bad(resource_directive(r#"#resource "C:/knob.png""#.into()));
    }
}
