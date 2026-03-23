Rename the type parameter `T` to `U` in `parser.rs` and update all usages
across the module. The type parameter is used in the `Parser` struct, its
`impl` block, and in `helpers.rs` which imports and uses it.

Make sure all files compile after the rename.
