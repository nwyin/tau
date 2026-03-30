pub mod permissions;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
pub enum DialogKind {
    Commands,
    Models,
    Sessions,
    Permissions,
}
