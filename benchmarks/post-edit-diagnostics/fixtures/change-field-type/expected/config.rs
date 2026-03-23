/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config<'a> {
    pub name: &'a str,
    pub port: u16,
    pub debug: bool,
}

impl<'a> Config<'a> {
    pub fn new(name: &'a str, port: u16) -> Self {
        Config {
            name,
            port,
            debug: false,
        }
    }

    pub fn with_debug(mut self) -> Self {
        self.debug = true;
        self
    }

    pub fn display_name(&self) -> &str {
        self.name
    }
}
