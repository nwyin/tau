const SIDEBAR_WIDTH: usize = 30;
const COMPACT_WIDTH_BREAK: usize = 120;
const COMPACT_HEIGHT_BREAK: usize = 30;
const MAX_TEXT_WIDTH: usize = 120;
const EDITOR_MIN_HEIGHT: usize = 3;
const EDITOR_MAX_HEIGHT: usize = 15;

#[allow(dead_code)]
pub struct LayoutRects {
    pub header_h: usize,
    pub chat_h: usize,
    pub editor_h: usize,
    pub status_h: usize,
    pub sidebar_w: usize,
    pub chat_w: usize,
    /// Capped message width for readability
    pub msg_w: usize,
}

pub fn is_compact(width: usize, height: usize) -> bool {
    width < COMPACT_WIDTH_BREAK || height < COMPACT_HEIGHT_BREAK
}

pub fn compute_layout(
    width: usize,
    height: usize,
    compact: bool,
    editor_lines: usize,
) -> LayoutRects {
    let editor_h = editor_lines.clamp(EDITOR_MIN_HEIGHT, EDITOR_MAX_HEIGHT) + 1; // +1 for border
    let header_h = if compact { 1 } else { 0 };
    let status_h = 1;
    let sidebar_w = if compact { 0 } else { SIDEBAR_WIDTH };
    let chat_w = width.saturating_sub(sidebar_w);
    let chat_h = height.saturating_sub(editor_h + header_h + status_h + 1); // +1 separator
    let msg_w = chat_w.min(MAX_TEXT_WIDTH);

    LayoutRects {
        header_h,
        chat_h,
        editor_h,
        status_h,
        sidebar_w,
        chat_w,
        msg_w,
    }
}
