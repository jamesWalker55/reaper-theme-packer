use std::{path::PathBuf, str::FromStr};

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take, take_till, take_till1},
    character::complete::{alpha1, char, newline, space0, space1},
    combinator::{cut, opt, recognize},
    multi::{many0, many1, separated_list1},
    sequence::{delimited, pair, preceded, terminated, tuple, Tuple},
    Err, IResult, Parser, Slice,
};
use nom_locate::LocatedSpan;
use relative_path::RelativePathBuf;
use serde::{Serialize, Serializer};
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
    #[error("hash char is not walter code `{0}`")]
    NonWALTERHash(I),
    #[error("incorrect #include syntax `{0}`")]
    MalformedIncludeDirective(I),
    #[error("incorrect #resource syntax `{0}`")]
    MalformedResourceDirective(I),
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
        opt(escaped(
            take_till1(|x| x == '"' || x == '\\'),
            '\\',
            take(1usize),
        )),
        char('"'),
    ))(input)?;

    let parsed_string: String = serde_json::from_str(raw_string.as_ref())
        .or(Err(Err::Failure(ParseError::MalformedString(raw_string))))?;

    Ok((rest, (parsed_string, raw_string)))
}

fn comment(input: Input) -> Result {
    recognize(preceded(char(';'), take_till(|x| x == '\n')))(input)
}

fn serialise_relpathbuf<S>(
    dest: &RelativePathBuf,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(dest.as_str())
}

fn serialise_pattern<S>(
    pattern: &glob::Pattern,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(pattern.as_str())
}

#[derive(Debug, Serialize)]
enum Directive<'a> {
    #[serde(serialize_with = "serialise_relpathbuf")]
    Include(RelativePathBuf),
    Resource {
        #[serde(serialize_with = "serialise_pattern")]
        pattern: glob::Pattern,
        #[serde(serialize_with = "serialise_relpathbuf")]
        dest: RelativePathBuf,
    },
    Unknown {
        #[serde(serialize_with = "serialise_span")]
        name: Input<'a>,
        #[serde(serialize_with = "serialise_span")]
        contents: Input<'a>,
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
    let (rest, tag) = tag("#include")(input)?;
    let (rest, (path, _raw_string)) = delimited(space1, relative_path_string, space0)(rest)
        .map_err(|err| {
            if matches!(err, Err::Failure(_)) {
                err
            } else {
                Err::Failure(ParseError::MalformedIncludeDirective(tag))
            }
        })?;

    Ok((rest, Directive::Include(path)))
}

fn resource_directive(input: Input) -> Result<Directive> {
    let (rest, tag) = tag("#resource")(input)?;
    let (rest, (dest, (pattern, raw_pattern))) = delimited(
        space1,
        tuple((
            opt(terminated(
                relative_path_string,
                tuple((space0, char(':'), space0)),
            )),
            relative_path_string,
        )),
        space0,
    )(rest)
    .map_err(|err| {
        if matches!(err, Err::Failure(_)) {
            err
        } else {
            Err::Failure(ParseError::MalformedResourceDirective(tag))
        }
    })?;

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
    let (rest, (_, name, contents)) = tuple((char('#'), alpha1, take_till(|x| x == '\n')))(input)?;

    Ok((rest, Directive::Unknown { name, contents }))
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
        return Err(Err::Error(ParseError::NonWALTERHash(result.1)));
    }
    let dir = directive(input);
    if dir.is_ok() {
        // this hash belongs to a directive, not WALTER code
        return Err(Err::Error(ParseError::NonWALTERHash(result.1)));
    }
    if let Err(Err::Failure(_)) = dir {
        // this hash belongs to a failed and locked-in directive, not WALTER code
        return Err(Err::Error(ParseError::NonWALTERHash(result.1)));
    }

    Ok(result)
}

fn walter_code(input: Input) -> Result {
    recognize(many1(alt((
        take_till1(|x| x == '\n' || x == '#' || x == ';'),
        allowed_walter_hash_char,
    ))))(input)
}

fn serialise_span<S>(input: &Input, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(input.as_ref())
}

#[derive(Debug, Serialize)]
enum RtconfigContent<'a> {
    Newline,
    #[serde(serialize_with = "serialise_span")]
    Code(Input<'a>),
    #[serde(serialize_with = "serialise_span")]
    Expression(Input<'a>),
    #[serde(serialize_with = "serialise_span")]
    Comment(Input<'a>),
    #[serde(with = "serde_yaml::with::singleton_map")]
    Directive(Directive<'a>),
}

/// Parse a single commentless "line" of a rtconfig.txt. \*
///
/// Each line is separated by one or more newlines.
///
/// _(\* The exception is expressions, which can span multiple lines)_
fn rtconfig_line_commentless(input: Input) -> Result<Vec<RtconfigContent>> {
    // the line (excluding comments)
    alt((
        // a standard line of code
        many1(alt((
            walter_code.map(|x| RtconfigContent::Code(x)),
            // an expression can span multiple lines, this is intentional
            expression.map(|x| RtconfigContent::Expression(x)),
        ))),
        // a directive
        directive.map(|x| vec![RtconfigContent::Directive(x)]),
    ))(input)
}

/// Parse a single "line" of a rtconfig.txt. \*
///
/// Each line is separated by one or more newlines.
///
/// _(\* The exception is expressions, which can span multiple lines)_
fn rtconfig_line(input: Input) -> Result<Vec<RtconfigContent>> {
    alt((
        pair(rtconfig_line_commentless, opt(comment)).map(|(mut contents, cmt)| {
            if let Some(cmt) = cmt {
                contents.push(RtconfigContent::Comment(cmt));
            }
            contents
        }),
        comment.map(|x| vec![RtconfigContent::Comment(x)]),
    ))(input)
}

fn rtconfig_newline(input: Input) -> Result<RtconfigContent> {
    newline.map(|_| RtconfigContent::Newline).parse(input)
}

fn rtconfig(input: Input) -> Result<Vec<RtconfigContent>> {
    tuple((
        many0(rtconfig_newline),
        separated_list1(many1(rtconfig_newline), rtconfig_line),
        many0(rtconfig_newline),
    ))
    .map(|(nl1, contents, nl2)| {
        nl1.into_iter()
            .chain(contents.into_iter().flatten())
            .chain(nl2.into_iter())
            .collect::<Vec<_>>()
    })
    .parse(input)
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use nom::{combinator::all_consuming, Finish};

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
    fn unit_tests() {
        ok(string(r#""this is a string!""#.into()));
        ok(string(r#""""#.into()));
        ok(string(r#""I will say \"Hello, world!\", ok?""#.into()));
        bad(string(
            r#""I'm gonna asd\a\sd\a\\asdasad\\as\d\as\d\a""#.into(),
        ));
        ok(string(r#""I'm gonna \\n fake new line""#.into()));
        bad(string(r#""I'm gonna \""#.into()));

        ok(comment(r#"; this is a comment, ashuidasj"#.into()));
        ok(comment(r#"; this is a comment, ashuidasj  "#.into()));

        ok(include_directive(
            r#"#include "./test/tcp.rtconfig.txt""#.into(),
        ));
        ok(include_directive(
            r#"#include    "./test/tcp.rtconfig.txt"  "#.into(),
        ));
        bad(include_directive(
            r#"#include    "C:/test/tcp.rtconfig.txt"  "#.into(),
        ));

        ok(unknown_directive(
            r#"#include "./test/tcp.rtconfig.txt""#.into(),
        ));
        ok(unknown_directive(
            r#"#include    "./test/tcp.rtconfig.txt"  "#.into(),
        ));
        ok(unknown_directive(
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

    #[test]
    fn test_rtconfig() {
        let text = std::fs::read_to_string("test/test.rtconfig.txt").unwrap();

        let result = rtconfig(text.as_str().into()).finish();
        match result {
            Ok((rest, contents)) => {
                std::fs::write("./parsed.yaml", serde_yaml::to_string(&contents).unwrap()).unwrap();

                if rest.len() > 0 {
                    panic!(
                        "failed to parse rest of document!\nLine {} Column {}",
                        rest.location_line(),
                        rest.get_column()
                    );
                }
            }
            Err(result) => {
                panic!("failed to even start parsing document! {:?}", result);
            }
        }
    }
}
