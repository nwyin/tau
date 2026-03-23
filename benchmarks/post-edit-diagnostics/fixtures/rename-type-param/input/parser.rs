use std::marker::PhantomData;

pub struct Parser<T> {
    buffer: Vec<u8>,
    position: usize,
    _marker: PhantomData<T>,
}

impl<T> Parser<T> {
    pub fn new() -> Self {
        Parser {
            buffer: Vec::new(),
            position: 0,
            _marker: PhantomData,
        }
    }

    pub fn parse(&mut self, input: &[u8]) -> Option<T>
    where
        T: Default,
    {
        self.buffer.extend_from_slice(input);
        self.position = 0;
        Some(T::default())
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.position = 0;
    }
}

pub fn create_parser<T>() -> Parser<T> {
    Parser::new()
}
