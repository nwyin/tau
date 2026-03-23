use crate::parser::{create_parser, Parser};

pub fn make_string_parser() -> Parser<String> {
    create_parser::<String>()
}

pub fn parse_default<U: Default>(data: &[u8]) -> Option<U> {
    let mut parser: Parser<U> = create_parser();
    parser.parse(data)
}

pub fn reset_parser<U>(parser: &mut Parser<U>) {
    parser.reset();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_parser() {
        let _p = make_string_parser();
    }

    #[test]
    fn test_parse_default() {
        let result = parse_default::<String>(b"hello");
        assert!(result.is_some());
    }
}
