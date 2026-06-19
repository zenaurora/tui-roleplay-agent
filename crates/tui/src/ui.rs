//! UI rendering with ratatui.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

/// Draw the entire UI.
pub fn draw(f: &mut Frame, app: &App) {
    // Main layout: top bar, middle (chat + sidebar), bottom input
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Story info bar
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
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
        } else if msg.is_user {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };

        let header_style = if msg.is_user {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else if msg.is_system {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
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
                format!("  {}", text_line),
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

    // Calculate scroll: account for line wrapping when estimating total visual lines
    let inner_height = area.height.saturating_sub(2) as usize; // subtract border
    let inner_width = area.width.saturating_sub(2) as usize; // subtract border

    // Estimate actual visual lines by accounting for text wrapping
    let total_visual_lines: usize = text
        .lines
        .iter()
        .map(|line| {
            let line_width: usize = line.spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
            if inner_width == 0 {
                1
            } else {
                // Each line takes at least 1 visual line, plus wraps
                ((line_width.max(1)) + inner_width - 1) / inner_width
            }
        })
        .sum();

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
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue))
                .title(" Input (Enter to send, Ctrl+C to quit) "),
        );
    f.render_widget(input, area);

    // Calculate visual cursor position accounting for multi-byte/double-width chars
    let text_before_cursor: String = app.input.chars().take(app.cursor_position).collect();
    let display_width = UnicodeWidthStr::width(text_before_cursor.as_str()) as u16;

    f.set_cursor_position((
        area.x + display_width + 1,
        area.y + 1,
    ));
}
