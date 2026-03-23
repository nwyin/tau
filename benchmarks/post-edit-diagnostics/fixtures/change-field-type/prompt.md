Change the `name` field in the `Config` struct from `String` to `&str` with
a lifetime parameter. The struct is defined in `config.rs` and used in
`loader.rs` and `display.rs`.

Add the necessary lifetime annotations to `Config`, its `impl` block, and
all functions that use it. Make sure all files compile with `cargo check`.
