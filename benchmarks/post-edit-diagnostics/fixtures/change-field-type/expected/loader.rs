use crate::config::Config;

pub fn load_config(raw: &str) -> Config<'_> {
    let name = raw.splitn(2, ':').next().unwrap_or("default");
    let port: u16 = raw
        .splitn(2, ':')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    Config::new(name, port)
}

pub fn load_debug_config(raw: &str) -> Config<'_> {
    load_config(raw).with_debug()
}
