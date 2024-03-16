use nom::{
    bytes::complete::{escaped, take, take_till, take_till1},
    character::complete::char,
    combinator::recognize,
    error::Error,
    sequence::{delimited, preceded},
    Err, IResult,
};

fn string(input: &str) -> IResult<&str, String> {
    let (rest, raw_string) = recognize(delimited(
        char('"'),
        escaped(take_till1(|x| x == '"' || x == '\\'), '\\', take(1usize)),
        char('"'),
    ))(input)?;

    let parsed_string: String = serde_json::from_str(raw_string)
        .map_err(|_| Err::Failure(Error::new(input, nom::error::ErrorKind::Escaped)))?;

    Ok((rest, parsed_string))
}

fn line_comment(input: &str) -> IResult<&str, &str> {
    recognize(preceded(char(';'), take_till(|x| x == '\n')))(input)
}

#[test]
fn parse_string() {
    string(r#""this is a string! I will say \"Hello, world!\", ok?""#).unwrap();
    line_comment(r#"; this is a comment, ashuidasj"#).unwrap();
}
