# Bugfix: 中文输入（IME）导致 Panic

## 问题现象

当使用输入法（IME）在输入框中输入中文或其他多字节字符时，应用 panic 并报错：

```
thread 'main' panicked at 'byte index 1 is not a char boundary; ...'
```

## 根本原因

`cursor_position` 字段同时被当作**字符索引**（每次按键加 1）和**字节偏移**（直接传给 `String::insert()` 和 `String::remove()`）使用。

在 Rust 中，`String` 是 UTF-8 编码的。ASCII 字符占 1 字节，但中文字符占 **3 字节**。这两种索引系统仅在纯 ASCII 时才一致：

```
输入: "你好"
         ^
字节布局: [0xE4, 0xBD, 0xA0,        0xE5, 0xA5, 0xBD]
              |--- '你' (3字节) ---| |--- '好' (3字节) ---|

字符索引:   0                        1
字节索引:   0                        3
```

输入 '你' 后，`cursor_position = 1`。接下来的 `String::insert(1, c)` 尝试在字节偏移 1 处插入 —— 这正好是 **UTF-8 序列的中间** —— 从而导致 panic。

## 三个子 Bug

### 1. `String::insert` / `String::remove` —— 字节索引与字符索引混淆

```rust
// BEFORE（CJK 字符会 panic）
self.input.insert(self.cursor_position, c);   // 期望字节索引！
self.input.remove(self.cursor_position);      // 期望字节索引！
```

### 2. `String::len()` —— 返回字节数，而非字符数

```rust
// BEFORE（边界检查使用了字节长度）
if self.cursor_position < self.input.len() { ... }  // 对 CJK 是错误的

// 输入 "你好" 后，input.len() = 6，但实际只有 2 个字符
```

### 3. 光标渲染 —— 字符数与显示宽度混淆

中文字符在终端中是**双宽字符**（每个占 2 列）。用字符数作为像素偏移会导致光标位置错误：

```
终端显示: [你好abc]
显示宽度:    2  2  1 1 1  = 7 列
字符数:      1  2  3 4 5  = 5 个字符
```

## 修复方案

### 字符感知索引的辅助方法

```rust
/// 将字符索引转换为字节索引。
fn char_to_byte_index(&self, char_idx: usize) -> usize {
    self.input
        .char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(self.input.len())
}

/// 获取字符数（而非字节数）。
fn input_char_count(&self) -> usize {
    self.input.chars().count()
}
```

### 修复插入操作

```rust
// AFTER
let byte_idx = self.char_to_byte_index(self.cursor_position);
self.input.insert(byte_idx, c);
self.cursor_position += 1;
```

### 修复退格删除

```rust
// AFTER —— 在字节位置删除完整的字符
self.cursor_position -= 1;
let byte_idx = self.char_to_byte_index(self.cursor_position);
let ch = self.input[byte_idx..].chars().next().unwrap();
self.input.drain(byte_idx..byte_idx + ch.len_utf8());
```

### 修复边界检查

```rust
// AFTER —— 与字符数比较，而非字节长度
if self.cursor_position < self.input_char_count() { ... }
```

### 使用 `unicode-width` 修复光标渲染

```rust
use unicode_width::UnicodeWidthStr;

// 计算终端光标的视觉列位置
let text_before_cursor: String = app.input.chars().take(app.cursor_position).collect();
let display_width = UnicodeWidthStr::width(text_before_cursor.as_str()) as u16;

f.set_cursor_position((
    area.x + display_width + 1,
    area.y + 1,
));
```

## 关键概念

### Rust String 有三种不同的"长度"

| 方法 | 含义 | "你好abc" |
|--------|---------|-----------|
| `.len()` | 字节数（UTF-8） | 9 |
| `.chars().count()` | 字符数（Unicode 标量值） | 5 |
| `UnicodeWidthStr::width()` | 终端显示列数 | 7 |

### 何时使用哪种

- **字节索引** — `String::insert()`、`String::remove()`、切片 `&s[a..b]`
- **字符索引** — 逻辑光标位置、面向用户的"字符数"
- **显示宽度** — 终端光标列定位

### 转换模式

```rust
// 字符索引 → 字节索引（用于 String 方法）
let byte_idx = s.char_indices().nth(char_idx).map(|(i,_)| i).unwrap_or(s.len());

// 字节索引 → 字符索引（如需要）
let char_idx = s[..byte_idx].chars().count();
```

## 经验教训

在 Rust 中，**永远不要假设字符索引 == 字节索引**。处理用户文本输入时：

1. 光标存储为**字符索引**（逻辑位置）
2. 仅在调用 `String` 变更方法时转换为**字节索引**
3. 终端光标渲染使用**显示宽度**（通过 `unicode-width`）
4. 边界检查使用 `.chars().count()` 而非 `.len()`

这是 Rust 字符串安全的基本规则 —— 类型系统会阻止你索引到字符中间（panic），但你仍需自行跟踪两套坐标系统。
# Bugfix: Chinese Input (IME) Causes Panic

## Problem

When using an input method (IME) to type Chinese or other multi-byte characters in the input box, the app panics with:

```
thread 'main' panicked at 'byte index 1 is not a char boundary; ...'
```

## Root Cause

The `cursor_position` field was treated as both a **character index** (incremented by 1 per keystroke) and a **byte offset** (passed directly to `String::insert()` and `String::remove()`).

In Rust, `String` is UTF-8 encoded. ASCII characters are 1 byte, but Chinese characters are **3 bytes**. These two index systems only align for pure ASCII:

```
Input: "你好"
         ^
Byte layout: [0xE4, 0xBD, 0xA0,        0xE5, 0xA5, 0xBD]
              |--- '你' (3 bytes) ---| |--- '好' (3 bytes) ---|

char index:   0                        1
byte index:   0                        3
```

After typing '你', `cursor_position = 1`. The next `String::insert(1, c)` tries to insert at byte offset 1 — which is **the middle of a UTF-8 sequence** — causing the panic.

## The Three Sub-Bugs

### 1. `String::insert` / `String::remove` — byte vs char index

```rust
// BEFORE (panics on CJK)
self.input.insert(self.cursor_position, c);   // expects byte index!
self.input.remove(self.cursor_position);      // expects byte index!
```

### 2. `String::len()` — returns bytes, not chars

```rust
// BEFORE (boundary check uses byte length)
if self.cursor_position < self.input.len() { ... }  // wrong for CJK

// After typing "你好", input.len() = 6, but there are only 2 characters
```

### 3. Cursor rendering — char count vs display width

Chinese characters are **double-width** in terminal (each occupies 2 columns). Using char count as pixel offset misplaces the cursor:

```
Terminal display: [你好abc]
Display width:    2  2  1 1 1  = 7 columns
Char count:       1  2  3 4 5  = 5 characters
```

## Fix

### Helper methods for char-aware indexing

```rust
/// Convert char index to byte index.
fn char_to_byte_index(&self, char_idx: usize) -> usize {
    self.input
        .char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(self.input.len())
}

/// Get character count (not byte count).
fn input_char_count(&self) -> usize {
    self.input.chars().count()
}
```

### Fixed insert

```rust
// AFTER
let byte_idx = self.char_to_byte_index(self.cursor_position);
self.input.insert(byte_idx, c);
self.cursor_position += 1;
```

### Fixed backspace

```rust
// AFTER — remove the full char at the byte position
self.cursor_position -= 1;
let byte_idx = self.char_to_byte_index(self.cursor_position);
let ch = self.input[byte_idx..].chars().next().unwrap();
self.input.drain(byte_idx..byte_idx + ch.len_utf8());
```

### Fixed boundary check

```rust
// AFTER — compare against char count, not byte length
if self.cursor_position < self.input_char_count() { ... }
```

### Fixed cursor rendering with `unicode-width`

```rust
use unicode_width::UnicodeWidthStr;

// Calculate visual column position for the terminal cursor
let text_before_cursor: String = app.input.chars().take(app.cursor_position).collect();
let display_width = UnicodeWidthStr::width(text_before_cursor.as_str()) as u16;

f.set_cursor_position((
    area.x + display_width + 1,
    area.y + 1,
));
```

## Key Concepts

### Rust String has 3 different "lengths"

| Method | Meaning | "你好abc" |
|--------|---------|-----------|
| `.len()` | Byte count (UTF-8) | 9 |
| `.chars().count()` | Character count (Unicode scalar) | 5 |
| `UnicodeWidthStr::width()` | Terminal display columns | 7 |

### When to use which

- **Byte index** — `String::insert()`, `String::remove()`, slicing `&s[a..b]`
- **Char index** — logical cursor position, user-facing "character count"
- **Display width** — terminal cursor column placement

### The conversion pattern

```rust
// char index → byte index (for String methods)
let byte_idx = s.char_indices().nth(char_idx).map(|(i,_)| i).unwrap_or(s.len());

// byte index → char index (if needed)
let char_idx = s[..byte_idx].chars().count();
```

## Lesson

In Rust, **never assume char index == byte index**. Any time you handle user text input:

1. Store cursor as **char index** (logical position)
2. Convert to **byte index** only when calling `String` mutation methods
3. Use **display width** (via `unicode-width`) for terminal cursor rendering
4. Use `.chars().count()` instead of `.len()` for boundary checks

This is a fundamental Rust string safety rule — the type system prevents you from indexing into the middle of a char (panic), but you must still track the two coordinate systems yourself.
