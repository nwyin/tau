/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub name: String,
    pub port: u16,
    pub debug: bool,
}

impl Config {
    pub fn new(name: String, port: u16) -> Self {
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
        &self.name
    }
}
