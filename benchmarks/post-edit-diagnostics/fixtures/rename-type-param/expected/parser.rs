use std::marker::PhantomData;

pub struct Parser<U> {
    buffer: Vec<u8>,
    position: usize,
    _marker: PhantomData<U>,
}

impl<U> Parser<U> {
    pub fn new() -> Self {
        Parser {
            buffer: Vec::new(),
            position: 0,
            _marker: PhantomData,
        }
    }

    pub fn parse(&mut self, input: &[u8]) -> Option<U>
    where
        U: Default,
    {
        self.buffer.extend_from_slice(input);
        self.position = 0;
        Some(U::default())
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.position = 0;
    }
}

pub fn create_parser<U>() -> Parser<U> {
    Parser::new()
}
