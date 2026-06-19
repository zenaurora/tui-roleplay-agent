# Bugfix: 聊天面板文本不换行

## 问题现象

当终端窗口较窄时，TUI 中的聊天消息会被截断而非自动换行。超出面板宽度的长消息内容直接不可见。

## 根本原因

原始代码使用 ratatui 的 `List` 组件，每条消息对应一个 `Line`：

```rust
// BEFORE（有 bug）
let content = Line::from(vec![
    Span::styled(format!("[{}] ", msg.character_name), header_style),
    Span::styled(&msg.content, style),
]);
ListItem::new(content)

// ...
let chat = List::new(messages).block(/* ... */);
```

**为什么不会换行：**

- ratatui 中的 `Line` = 恰好一个终端行，永远不会断行。
- `List` 将每个 `ListItem` 渲染在单行中，没有自动换行能力。
- 如果内容超出组件宽度，会被静默截断。

这是设计如此 —— `List` 适用于短的、固定高度的条目（如文件名、菜单选项），而非多行流动文本。

## 修复方案

用 `Paragraph` + `Wrap` 替换 `List`：

```rust
// AFTER（修复后）
let mut lines: Vec<Line> = Vec::new();

for msg in &app.messages {
    // 标题独占一行
    lines.push(Line::from(Span::styled(
        format!("[{}]", msg.character_name),
        header_style,
    )));

    // 内容行（缩进）
    for text_line in msg.content.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {}", text_line),
            style,
        )));
    }

    // 空行分隔
    lines.push(Line::from(""));
}

let text = Text::from(lines);

let chat = Paragraph::new(text)
    .wrap(Wrap { trim: false })  // <-- 启用自动换行
    .scroll((scroll, 0))        // <-- 垂直滚动支持
    .block(/* ... */);
```

## 关键概念

### ratatui 中 `List` 与 `Paragraph` 的区别

| 特性 | `List` | `Paragraph` |
|---------|--------|-------------|
| 自动换行 | 否 | 是（通过 `.wrap()`） |
| 滚动 | 内置选择状态 | 手动通过 `.scroll((y, x))` |
| 适用场景 | 短的固定条目 | 流动文本内容 |

### `Wrap { trim: false }`

- `trim: true` — 去除换行后的前导空白（默认）
- `trim: false` — 保留所有空白，对缩进内容很重要

### 滚动计算

由于 `Paragraph` 不会自动滚动到底部，需要手动计算：

```rust
let inner_height = area.height.saturating_sub(2) as usize; // 减去边框
let total_lines = text.lines.len();
let scroll = if app.scroll_offset == 0 {
    // 自动滚动：显示最新消息
    total_lines.saturating_sub(inner_height) as u16
} else {
    // 用户通过 PageUp 向上滚动
    total_lines.saturating_sub(inner_height)
        .saturating_sub(app.scroll_offset as usize) as u16
};
```

## 经验教训

选择 ratatui 文本显示组件时：

- **固定高度条目**（菜单、文件列表）→ `List`
- **需要换行的变长文本**（聊天、日志、文档）→ 使用 `Wrap` 的 `Paragraph`

务必检查内容是否会超出组件宽度。如果会，`List` 会静默截断 —— 没有错误、没有警告，只是文本不可见。
