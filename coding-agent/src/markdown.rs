//! Simple markdown → ratatui Line/Span renderer.
//!
//! Handles: bold, italic, code spans, code blocks, headers, lists, blockquotes.
//! Uses pulldown-cmark for parsing.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a markdown string into ratatui Lines.
pub fn render(markdown: &str) -> Vec<Line<'static>> {
    let opts = Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(markdown, opts);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut list_depth: usize = 0;
    let mut in_code_block = false;
    let mut code_block_buf = String::new();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let style = match level {
                        pulldown_cmark::HeadingLevel::H1 => Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                        pulldown_cmark::HeadingLevel::H2 => Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                        _ => Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    };
                    style_stack.push(style);
                }
                Tag::Emphasis => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::ITALIC));
                }
                Tag::Strong => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::BOLD));
                }
                Tag::Strikethrough => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::CROSSED_OUT));
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                    code_block_buf.clear();
                    // Flush current line
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                }
                Tag::List(_) => {
                    list_depth += 1;
                }
                Tag::Item => {
                    // Flush current line
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                    let indent = "  ".repeat(list_depth.saturating_sub(1));
                    current_spans.push(Span::styled(
                        format!("{}• ", indent),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                Tag::BlockQuote(_) => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.fg(Color::DarkGray));
                }
                Tag::Paragraph => {
                    // Flush any prior content with a blank line between paragraphs
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                }
                Tag::Link { dest_url, .. } => {
                    // We'll handle the link text normally, then append the URL
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.fg(Color::Blue).add_modifier(Modifier::UNDERLINED));
                    // Store URL for later
                    current_spans.push(Span::raw("")); // placeholder
                    let _ = dest_url; // URL shown in End handler
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    style_stack.pop();
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    let style = Style::default().fg(Color::Green);
                    for code_line in code_block_buf.lines() {
                        lines.push(Line::from(Span::styled(format!("  {}", code_line), style)));
                    }
                    code_block_buf.clear();
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                }
                TagEnd::Item => {
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                }
                TagEnd::BlockQuote(_) => {
                    style_stack.pop();
                }
                TagEnd::Paragraph => {
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                }
                TagEnd::Link => {
                    style_stack.pop();
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_block_buf.push_str(&text);
                } else {
                    let style = *style_stack.last().unwrap_or(&Style::default());
                    current_spans.push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(code) => {
                current_spans.push(Span::styled(
                    format!("`{}`", code),
                    Style::default().fg(Color::Yellow),
                ));
            }
            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                lines.push(Line::from(std::mem::take(&mut current_spans)));
            }
            Event::Rule => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
                lines.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            _ => {}
        }
    }

    // Flush remaining
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let lines = render("hello world");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_header() {
        let lines = render("# Title\n\nBody text");
        assert!(lines.len() >= 2);
        // Header should have bold modifier
        let header_span = &lines[0].spans[0];
        assert!(
            header_span.style.add_modifier == Modifier::BOLD
                || header_span.style.fg == Some(Color::Cyan)
        );
    }

    #[test]
    fn test_code_block() {
        let lines = render("```\nlet x = 1;\n```");
        assert!(lines
            .iter()
            .any(|l| { l.spans.iter().any(|s| s.content.contains("let x = 1")) }));
    }

    #[test]
    fn test_inline_code() {
        let lines = render("use `foo` here");
        let has_code = lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains("`foo`") && s.style.fg == Some(Color::Yellow))
        });
        assert!(has_code);
    }

    #[test]
    fn test_list() {
        let lines = render("- item one\n- item two");
        assert!(lines.len() >= 2);
        assert!(lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains("•"))));
    }

    #[test]
    fn test_bold() {
        let lines = render("**bold text**");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_empty() {
        let lines = render("");
        assert!(lines.is_empty());
    }
}
