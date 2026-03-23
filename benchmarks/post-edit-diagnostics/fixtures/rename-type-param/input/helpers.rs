use crate::parser::{create_parser, Parser};

pub fn make_string_parser() -> Parser<String> {
    create_parser::<String>()
}

pub fn parse_default<T: Default>(data: &[u8]) -> Option<T> {
    let mut parser: Parser<T> = create_parser();
    parser.parse(data)
}

pub fn reset_parser<T>(parser: &mut Parser<T>) {
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
