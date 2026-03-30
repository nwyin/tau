// Permission dialog rendering and input handling is inline in model.rs:
// - PendingPermission struct holds the tool name, description, and response channel
// - view_chat() renders the permission modal as a bordered box
// - update() intercepts all keys while perm_pending is Some
// - Ctrl-C during permission denies and aborts
