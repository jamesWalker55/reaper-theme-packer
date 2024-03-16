use std::{path::PathBuf, str::FromStr};

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take, take_till, take_till1},
    character::complete::{alpha1, char, newline, space0, space1},
    combinator::{opt, recognize},
    multi::{many0, many1},
    sequence::{delimited, preceded, terminated, tuple, Tuple},
    Err, IResult, Parser, Slice,
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
    #[error("hash char is walter code `{0}`")]
    WALTERHash(I),
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

fn string(input: Input) -> Result<(String, Input)> {
    let (rest, raw_string) = recognize(delimited(
        char('"'),
        escaped(take_till1(|x| x == '"' || x == '\\'), '\\', take(1usize)),
        char('"'),
    ))(input)?;

    let parsed_string: String = serde_json::from_str(raw_string.as_ref())
        .or(Err(Err::Failure(ParseError::MalformedString(raw_string))))?;

    Ok((rest, (parsed_string, raw_string)))
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

fn relative_path_string(input: Input) -> Result<(RelativePathBuf, Input)> {
    let (rest, (parsed_string, raw_string)) = string(input)?;

    // convert to PathBuf, may be absolute
    let path = PathBuf::from_str(parsed_string.as_str())
        .or(Err(Err::Failure(ParseError::MalformedPath(raw_string))))?;
    // convert to RelativePathBuf, must be relative now
    let path = RelativePathBuf::from_path(path)
        .or(Err(Err::Failure(ParseError::NotRelativePath(raw_string))))?;

    Ok((rest, (path, raw_string)))
}

fn include_directive(input: Input) -> Result<Directive> {
    let (rest, (_, _, (path, raw_string), _)) =
        (tag("#include"), space1, relative_path_string, space0).parse(input)?;

    Ok((rest, Directive::Include(path)))
}

fn resource_directive(input: Input) -> Result<Directive> {
    let (rest, (_, _, dest, (pattern, raw_pattern), _)) = (
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
    let dest = dest
        .and_then(|x| Some(x.0))
        .unwrap_or(RelativePathBuf::from("."));

    // parse pattern
    let pattern = glob::Pattern::new(pattern.as_str()).or(Err(Err::Failure(
        ParseError::InvalidGlobPattern(raw_pattern),
    )))?;

    Ok((rest, Directive::Resource { pattern, dest }))
}

fn unknown_directive(input: Input) -> Result<Directive> {
    let (rest, result) = recognize(tuple((char('#'), alpha1, take_till(|x| x == '\n'))))(input)?;

    Err(Err::Failure(ParseError::UnknownDirective(result)))
}

fn directive(input: Input) -> Result<Directive> {
    alt((include_directive, resource_directive, unknown_directive))(input)
}

/// Recognise text with brace pairs. E.g. `"{ Hello! {Nested} }"` will return `"{ Hello! {Nested} }"`
fn brace_pair(input: Input) -> Result {
    recognize(tuple((
        char('{'),
        many0(alt((take_till1(|x| x == '{' || x == '}'), brace_pair))),
        char('}'),
    )))(input)
}

fn expression(input: Input) -> Result {
    let (rest, result) = preceded(char('#'), brace_pair)(input)?;

    // trim the left and right braces
    let result = result.slice(1..(result.len() - 1));

    Ok((rest, result))
}

fn allowed_walter_hash_char(input: Input) -> Result {
    // check that it begins with a hash sign
    let result = tag("#")(input)?;

    // disallow directives and expression
    let expr = expression(input);
    if expr.is_ok() {
        // this hash belongs to an expression, not WALTER code
        return Err(Err::Error(ParseError::WALTERHash(input)));
    }
    let dir = directive(input);
    if dir.is_ok() {
        // this hash belongs to a directive, not WALTER code
        return Err(Err::Error(ParseError::WALTERHash(input)));
    }

    Ok(result)
}

fn walter_code(input: Input) -> Result {
    recognize(many1(alt((
        take_till1(|x| x == '\n' || x == '#'),
        allowed_walter_hash_char,
    ))))(input)
}

enum RtconfigContent<'a> {
    Newline,
    Code(Input<'a>),
    Expression(Input<'a>),
    Directive(Directive),
}

fn rtconfig(input: Input) -> Result<Vec<RtconfigContent>> {
    let mut result: Vec<RtconfigContent> = vec![];
    let mut input = input;

    loop {
        // try to parse normal code
        let walter_line = many1(alt((
            walter_code.map(|x| RtconfigContent::Code(x)),
            // an expression can span multiple lines, this is intentional
            expression.map(|x| RtconfigContent::Expression(x)),
        )))(input);
        if let Ok((rest, mut contents)) = walter_line {
            result.append(&mut contents);
            input = rest;

            // end of this line, try to take a newline
            if let Ok((rest, _)) = newline::<LocatedSpan<&str>, ParseError<Input>>(input) {
                // successfully taken newline, time to parse the next line
                result.push(RtconfigContent::Newline);
                input = rest;
                continue;
            }

            // failed to take newline, this must be end of file
            return Ok((input, result));
        };

        if let Ok((rest, dir)) = directive(input) {
            result.push(RtconfigContent::Directive(dir));
            input = rest;

            // end of this line, try to take a newline
            if let Ok((rest, _)) = newline::<LocatedSpan<&str>, ParseError<Input>>(input) {
                // successfully taken newline, time to parse the next line
                result.push(RtconfigContent::Newline);
                input = rest;
                continue;
            }

            // failed to take newline, this must be end of file
            return Ok((input, result));
        }

        // failed to parse directive or normal code, this must be end of file
        return Ok((input, result));
    }
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

        ok(brace_pair(r#"{}"#.into()));
        ok(brace_pair(r#"{ 1 + 1 }"#.into()));
        ok(brace_pair(r#"{ apple }"#.into()));
        ok(brace_pair(r#"{ {a: 1, b: 2} }"#.into()));
        ok(brace_pair(r#"{ {a: 1, b: {c: 2, d: 3}} }"#.into()));
        ok(brace_pair("{ {a: 1,\n b: \n{c: \n2,\n \nd: 3}} }".into()));
        bad(brace_pair(r#"{ } }"#.into()));
        bad(brace_pair(r#"{ }{ }"#.into()));
        bad(brace_pair(r#"{ { }"#.into()));
        bad(brace_pair(r#""#.into()));

        ok(expression(r#"#{ {a: 1, b: {c: 2, d: 3}} }"#.into()));
        bad(expression(r#"# { {a: 1, b: {c: 2, d: 3}} }"#.into()));
        bad(expression(r#""#.into()));

        ok(walter_code("hello world".into()));
        ok(walter_code("hello world # ibhsdkasj".into()));
        bad(walter_code("hello world #{ 1+1 } ibhsdkasj".into()));
        ok((walter_code, expression, walter_code).parse("hello world #{ 1+1 } ibhsdkasj".into()));
        ok((walter_code, expression, walter_code)
            .parse("hello world #{ 1\n+\n1 } ibhsdkasj".into()));
        bad((walter_code, expression, walter_code)
            .parse("hello \nworld #{ 1\n+\n1 } ibhsdkasj".into()));
        bad(walter_code("".into()));
    }
}
