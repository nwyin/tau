use crate::config::Config;

pub fn format_config(config: &Config<'_>) -> String {
    let debug_str = if config.debug { " [DEBUG]" } else { "" };
    format!(
        "{}:{}{debug_str}",
        config.display_name(),
        config.port,
    )
}

pub fn print_config(config: &Config<'_>) {
    println!("{}", format_config(config));
}
