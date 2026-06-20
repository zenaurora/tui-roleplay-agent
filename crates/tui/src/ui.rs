//! UI rendering with ratatui.

use std::collections::VecDeque;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, StyledGrapheme, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;

/// Draw the entire UI.
pub fn draw(f: &mut Frame, app: &App) {
    // Main layout: top bar, middle (chat + sidebar), bottom input
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Story info bar
            Constraint::Min(10),   // Chat + sidebar
            Constraint::Length(3), // Input
        ])
        .split(f.area());

    draw_story_bar(f, app, main_chunks[0]);

    // Middle: chat panel + character sidebar
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(75), // Chat
            Constraint::Percentage(25), // Sidebar
        ])
        .split(main_chunks[1]);

    draw_chat(f, app, middle_chunks[0]);
    draw_sidebar(f, app, middle_chunks[1]);
    draw_input(f, app, main_chunks[2]);
}

/// Draw the story info bar at the top.
fn draw_story_bar(f: &mut Frame, app: &App, area: Rect) {
    let info = format!(
        " {} | Scene: {} | Messages: {}{}",
        app.story_title,
        app.scene_name,
        app.messages.len(),
        if app.is_loading { " | Thinking..." } else { "" }
    );

    let paragraph = Paragraph::new(info)
        .style(Style::default().fg(Color::Cyan))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Story "),
        );
    f.render_widget(paragraph, area);
}

/// Draw the main chat panel.
fn draw_chat(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        let style = if msg.is_system {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC)
        } else if msg.is_user {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };

        let header_style = if msg.is_user {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else if msg.is_system {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };

        // Header line with character name
        let header = Line::from(Span::styled(
            format!("[{}]", msg.character_name),
            header_style,
        ));
        lines.push(header);

        // Content lines - split by newline so each paragraph is its own Line
        // (Paragraph + Wrap will handle wrapping within each Line)
        for text_line in msg.content.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", add_cjk_wrap_hints(text_line)),
                style,
            )));
        }
        // If content is empty, still show an empty line
        if msg.content.is_empty() {
            lines.push(Line::from(Span::styled("  ", style)));
        }

        // Blank separator between messages
        lines.push(Line::from(""));
    }

    let text = Text::from(lines);

    // Calculate scroll in rendered visual rows, matching ratatui's word wrapping.
    let inner_height = area.height.saturating_sub(2) as usize; // subtract border
    let inner_width = area.width.saturating_sub(2) as usize; // subtract border
    let total_visual_lines = wrapped_line_count(&text, inner_width);

    let scroll = if app.scroll_offset == 0 {
        // Auto-scroll to bottom
        total_visual_lines.saturating_sub(inner_height) as u16
    } else {
        // User manually scrolled up
        total_visual_lines
            .saturating_sub(inner_height)
            .saturating_sub(app.scroll_offset as usize) as u16
    };

    let chat = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Chat "),
        );
    f.render_widget(chat, area);
}

fn wrapped_line_count(text: &Text<'_>, width: usize) -> usize {
    if width == 0 {
        return 0;
    }

    text.lines
        .iter()
        .map(|line| wrapped_symbols_line_count(line.styled_graphemes(Style::default()), width))
        .sum()
}

fn wrapped_symbols_line_count<'a>(
    symbols: impl IntoIterator<Item = StyledGrapheme<'a>>,
    width: usize,
) -> usize {
    let mut rows = 0;
    let mut line_width = 0;
    let mut word_width = 0;
    let mut whitespace_width = 0;
    let mut pending_whitespace: VecDeque<usize> = VecDeque::new();
    let mut pending_line_empty = true;
    let mut pending_word_empty = true;
    let mut non_whitespace_previous = false;

    for grapheme in symbols {
        let is_whitespace = is_wrappable_whitespace(grapheme.symbol);
        let symbol_width = UnicodeWidthStr::width(grapheme.symbol);

        if symbol_width > width {
            continue;
        }

        let word_found = non_whitespace_previous && is_whitespace;
        let untrimmed_overflow =
            pending_line_empty && word_width + whitespace_width + symbol_width > width;

        if word_found || untrimmed_overflow {
            if !pending_whitespace.is_empty() {
                pending_line_empty = false;
                line_width += whitespace_width;
            }

            if !pending_word_empty {
                pending_line_empty = false;
                line_width += word_width;
            }

            pending_word_empty = true;
            pending_whitespace.clear();
            whitespace_width = 0;
            word_width = 0;
        }

        let line_full = line_width >= width;
        let pending_word_overflow =
            symbol_width > 0 && line_width + whitespace_width + word_width >= width;

        if line_full || pending_word_overflow {
            let mut remaining_width = width.saturating_sub(line_width);
            rows += 1;
            line_width = 0;
            pending_line_empty = true;

            while let Some(width) = pending_whitespace.front().copied() {
                if width > remaining_width {
                    break;
                }
                whitespace_width -= width;
                remaining_width -= width;
                pending_whitespace.pop_front();
            }

            if is_whitespace && pending_whitespace.is_empty() {
                non_whitespace_previous = false;
                continue;
            }
        }

        if is_whitespace {
            whitespace_width += symbol_width;
            pending_whitespace.push_back(symbol_width);
        } else {
            word_width += symbol_width;
            pending_word_empty = false;
        }

        non_whitespace_previous = !is_whitespace;
    }

    if pending_line_empty && pending_word_empty && !pending_whitespace.is_empty() {
        rows += 1;
        pending_whitespace.clear();
    }

    if !pending_line_empty || !pending_whitespace.is_empty() || !pending_word_empty {
        rows += 1;
    }

    rows.max(1)
}

fn is_wrappable_whitespace(symbol: &str) -> bool {
    symbol == "\u{200b}" || symbol.chars().all(char::is_whitespace) && symbol != "\u{00a0}"
}

fn add_cjk_wrap_hints(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut previous_was_wide = false;

    for ch in text.chars() {
        let current_is_wide = is_wrappable_wide_char(ch);
        if previous_was_wide && current_is_wide {
            output.push('\u{200b}');
        }
        output.push(ch);
        previous_was_wide = current_is_wide;
    }

    output
}

fn is_wrappable_wide_char(ch: char) -> bool {
    !ch.is_whitespace() && UnicodeWidthChar::width(ch) == Some(2)
}

/// Draw the character sidebar.
fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .characters
        .iter()
        .map(|c| {
            let style = if c.is_active {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let content = Line::from(vec![
                Span::styled(&c.name, style.add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(&c.short_description, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(content)
        })
        .collect();

    let sidebar = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Characters "),
    );
    f.render_widget(sidebar, area);
}

/// Draw the input box at the bottom.
fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let (border_color, title) = if app.is_loading {
        (
            Color::DarkGray,
            " Input (locked — wait for NPCs to finish) ",
        )
    } else {
        (Color::Blue, " Input (Enter to send, Ctrl+C to quit) ")
    };

    let text_color = if app.is_loading {
        Color::DarkGray
    } else {
        Color::White
    };

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(text_color))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title),
        );
    f.render_widget(input, area);

    // Calculate visual cursor position accounting for multi-byte/double-width chars
    let text_before_cursor: String = app.input.chars().take(app.cursor_position).collect();
    let display_width = UnicodeWidthStr::width(text_before_cursor.as_str()) as u16;

    f.set_cursor_position((area.x + display_width + 1, area.y + 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, ChatMessage};
    use ratatui::{backend::TestBackend, layout::Position, Terminal};

    fn line_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[Position::new(x, y)].symbol())
            .collect::<String>()
    }

    #[test]
    fn wrapped_line_count_respects_word_boundaries() {
        let text = Text::from(Line::from("abcdef ghijkl mnopqr"));

        assert_eq!(wrapped_line_count(&text, 10), 3);
    }

    #[test]
    fn wrapped_line_count_handles_cjk_width() {
        let text = Text::from(Line::from(format!(
            "  {}",
            add_cjk_wrap_hints("你好世界你好")
        )));

        assert_eq!(wrapped_line_count(&text, 8), 2);
    }

    #[test]
    fn mixed_latin_cjk_wrap_uses_remaining_width() {
        let mut app = App::new("Story".to_string(), "Scene".to_string());
        app.messages.push(ChatMessage {
            character_name: "NPC".to_string(),
            content: "abcdefghijklmnopqrst 你好世界".to_string(),
            is_user: false,
            is_system: false,
        });

        let backend = TestBackend::new(42, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let screen_lines = (0..terminal.backend().buffer().area.height)
            .map(|y| line_text(terminal.backend().buffer(), y))
            .collect::<Vec<_>>();
        let mixed_line = screen_lines
            .iter()
            .find(|line| line.contains("abcdefghijklmnopqrst"))
            .expect("latin text should be visible");

        assert!(mixed_line.contains('你'), "\n{}", screen_lines.join("\n"));
    }

    #[test]
    fn auto_scroll_bottom_keeps_latest_message_visible() {
        let mut app = App::new("Story".to_string(), "Scene".to_string());
        app.messages.push(ChatMessage {
            character_name: "NPC".to_string(),
            content: "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu"
                .to_string(),
            is_user: false,
            is_system: false,
        });
        app.messages.push(ChatMessage {
            character_name: "NPC".to_string(),
            content: "最后一行必须可见".to_string(),
            is_user: false,
            is_system: false,
        });

        let backend = TestBackend::new(42, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let screen = (0..terminal.backend().buffer().area.height)
            .map(|y| line_text(terminal.backend().buffer(), y))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(screen.contains('最'), "\n{screen}");
        assert!(screen.contains('见'), "\n{screen}");
    }
}
