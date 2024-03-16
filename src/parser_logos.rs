use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
enum Token<'a> {
    #[token("\n")]
    Newline,

    #[regex(r";[^\n]*")]
    Comment(&'a str),

    #[regex(r"#\{.*\}")]
    Expression(&'a str),

    #[regex(r"#\w.*\n")]
    Directive(&'a str),

    #[regex(r"[^\n;#]+")]
    WalterCode(&'a str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_01() {
        let text = std::fs::read_to_string("test/test.rtconfig.txt").unwrap();
        for result in Token::lexer(&text) {
            match result {
                Ok(token) => println!("{:#?}", token),
                Err(e) => panic!("some error occurred: {:?}", e),
            }
        }
    }
}
