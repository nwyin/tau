use crate::config::Config;

pub fn load_config(raw: &str) -> Config {
    let mut parts = raw.splitn(2, ':');
    let name = parts.next().unwrap_or("default").to_string();
    let port: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    Config::new(name, port)
}

pub fn load_debug_config(raw: &str) -> Config {
    load_config(raw).with_debug()
}
