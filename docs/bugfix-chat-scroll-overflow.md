# Bugfix: 聊天面板滚动溢出（底部内容不可见）

## 问题现象

当聊天消息变多后，底部最新的消息被截断看不到。尽管代码已经实现了 auto-scroll 到底部的逻辑，但在中文长文本场景下，滚动位置总是差几行，导致最新内容被裁掉。

## 根本原因

之前的滚动计算直接使用 `text.lines.len()` 来估算内容总高度：

```rust
// BEFORE（有 bug）
let inner_height = area.height.saturating_sub(2) as usize;
let total_lines = text.lines.len();  // ← 只数 Line 对象个数
let scroll = if app.scroll_offset == 0 {
    total_lines.saturating_sub(inner_height) as u16
} else {
    total_lines.saturating_sub(inner_height)
        .saturating_sub(app.scroll_offset as usize) as u16
};
```

**问题在于：** `text.lines.len()` 统计的是 `Line` 对象数量，不是渲染后的**视觉行数**。

当使用 `Paragraph` + `Wrap { trim: false }` 时，一个 `Line` 如果内容超过面板宽度，会自动折行成多个视觉行。例如：

- 面板内宽 60 列
- 一行中文消息有 40 个汉字 = 显示宽度 80 列
- 实际渲染为 2 行，但 `lines.len()` 只计为 1

累积误差导致 auto-scroll 的偏移量永远不够大，底部内容就被裁掉了。

## 修复方案

用 `UnicodeWidthStr` 计算每个 `Line` 的实际显示宽度，再除以面板内宽，得到真实视觉行数：

```rust
// AFTER（修复后）
let inner_height = area.height.saturating_sub(2) as usize;
let inner_width = area.width.saturating_sub(2) as usize; // 减去边框

// 估算实际视觉行数（考虑 wrap 换行）
let total_visual_lines: usize = text
    .lines
    .iter()
    .map(|line| {
        let line_width: usize = line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        if inner_width == 0 {
            1
        } else {
            // 每行至少占 1 视觉行，超出部分按 inner_width 折行
            ((line_width.max(1)) + inner_width - 1) / inner_width
        }
    })
    .sum();

let scroll = if app.scroll_offset == 0 {
    total_visual_lines.saturating_sub(inner_height) as u16
} else {
    total_visual_lines
        .saturating_sub(inner_height)
        .saturating_sub(app.scroll_offset as usize) as u16
};
```

## 关键概念

### ratatui 的 `Paragraph` 滚动机制

`Paragraph` 的 `.scroll((row, col))` 是基于**渲染后的视觉行**来偏移的，不是基于 `Line` 对象。所以如果你用 `lines.len()` 来算 scroll 目标位置，结果一定会偏小。

### UnicodeWidthStr 计算显示宽度

中日韩（CJK）字符在终端占 2 列宽，ASCII 占 1 列：

```rust
use unicode_width::UnicodeWidthStr;

UnicodeWidthStr::width("hello");     // = 5
UnicodeWidthStr::width("你好世界");   // = 8（每个汉字 2 列）
```

### 向上取整计算折行数

```rust
// 一行占多少视觉行 = ceil(显示宽度 / 面板宽度)
let visual_rows = (line_width + inner_width - 1) / inner_width;
```

这是整数除法向上取整的标准写法。

## 经验教训

在 ratatui 中使用 `Paragraph` + `Wrap` 时：

1. **不要用 `lines.len()` 来计算滚动位置** — 它忽略了折行
2. **必须用 `UnicodeWidthStr`** 来正确估算 CJK 文本宽度
3. **scroll 的单位是视觉行**，必须和渲染引擎的折行逻辑保持一致
4. 这个问题在纯 ASCII 短文本下不明显，一旦有中文长段落就会暴露
