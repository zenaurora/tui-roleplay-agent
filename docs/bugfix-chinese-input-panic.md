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
