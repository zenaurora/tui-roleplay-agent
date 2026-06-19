# Bugfix: Chat Panel Text Not Wrapping

## Problem

When the terminal window is narrow, chat messages in the TUI get clipped/truncated instead of wrapping to the next line. Long messages are simply invisible beyond the panel width.

## Root Cause

The original code used ratatui's `List` widget with one `Line` per message:

```rust
// BEFORE (broken)
let content = Line::from(vec![
    Span::styled(format!("[{}] ", msg.character_name), header_style),
    Span::styled(&msg.content, style),
]);
ListItem::new(content)

// ...
let chat = List::new(messages).block(/* ... */);
```

**Why this doesn't wrap:**

- `Line` in ratatui = exactly one terminal row. It never breaks.
- `List` renders each `ListItem` in a single row. It has no word-wrap capability.
- If the content exceeds the widget width, it is silently clipped.

This is by design — `List` is meant for short, fixed-height items (like file names, menu options), not multi-line flowing text.

## Fix

Replace `List` with `Paragraph` + `Wrap`:

```rust
// AFTER (fixed)
let mut lines: Vec<Line> = Vec::new();

for msg in &app.messages {
    // Header on its own line
    lines.push(Line::from(Span::styled(
        format!("[{}]", msg.character_name),
        header_style,
    )));

    // Content lines (indented)
    for text_line in msg.content.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {}", text_line),
            style,
        )));
    }

    // Blank separator
    lines.push(Line::from(""));
}

let text = Text::from(lines);

let chat = Paragraph::new(text)
    .wrap(Wrap { trim: false })  // <-- enables word wrapping
    .scroll((scroll, 0))        // <-- vertical scroll support
    .block(/* ... */);
```

## Key Concepts

### `List` vs `Paragraph` in ratatui

| Feature | `List` | `Paragraph` |
|---------|--------|-------------|
| Word wrap | No | Yes (with `.wrap()`) |
| Scroll | Built-in selection state | Manual via `.scroll((y, x))` |
| Use case | Short fixed items | Flowing text content |

### `Wrap { trim: false }`

- `trim: true` — strips leading whitespace on wrapped lines (default)
- `trim: false` — preserves all whitespace, important for indented content

### Scroll calculation

Since `Paragraph` doesn't auto-scroll to the bottom, we compute it manually:

```rust
let inner_height = area.height.saturating_sub(2) as usize; // minus border
let total_lines = text.lines.len();
let scroll = if app.scroll_offset == 0 {
    // Auto-scroll: show the latest messages
    total_lines.saturating_sub(inner_height) as u16
} else {
    // User scrolled up via PageUp
    total_lines.saturating_sub(inner_height)
        .saturating_sub(app.scroll_offset as usize) as u16
};
```

## Lesson

When choosing a ratatui widget for text display:

- **Fixed-height items** (menus, file lists) → `List`
- **Variable-length text that needs wrapping** (chat, logs, documentation) → `Paragraph` with `Wrap`

Always check whether your content can exceed the widget width. If yes, `List` will silently clip it — no error, no warning, just invisible text.
