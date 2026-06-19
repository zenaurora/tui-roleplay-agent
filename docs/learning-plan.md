# 项目学习规划

## 项目当前完成度

| 模块 | 状态 | 代码量 | 核心功能 |
|------|------|--------|----------|
| core | 完成 | ~400行 | 类型定义、Trait、错误处理、配置 |
| llm | 完成 | ~385行 | OpenAI兼容客户端、流式SSE解析 |
| memory | 完成 | ~260行 | 对话记忆、滑动窗口、持久化 |
| agent | 完成 | ~255行 | StateGraph状态机、Chain管道、Tool注册 |
| roleplay | 完成 | ~355行 | 角色Agent、导演Agent、场景管理、回合管理 |
| tui | 完成 | ~560行 | ratatui界面、输入处理、命令解析 |
| main | 完成 | ~270行 | 组装所有模块、事件循环 |

**总计：~2850行 Rust代码，0个TODO/FIXME，所有功能已实现。**

---

## 第一阶段：整体架构理解（1-2天）

### 1.1 项目分层设计

```
main.rs（入口）
  ├── 加载配置 (story.toml)
  ├── 创建 LLM Client (OpenAiClient)
  ├── 创建 SceneManager + 角色
  ├── 创建 TurnManager + Director
  ├── 创建 ConversationMemory
  ├── 创建 CharacterAgents (每个角色一个)
  ├── 建立两对 channel
  │   ├── command_tx/rx  →  TUI → 命令处理
  │   └── event_tx/rx    →  命令处理 → TUI渲染
  ├── 启动命令处理任务 (tokio::spawn)
  └── 运行 TUI 事件循环
```

**核心问题：为什么用 channel 而不是共享状态？**

```
TUI渲染任务（主线程）                     命令处理任务（spawn线程）
        ↓                                       ↓
   接收 event_rx                         接收 command_rx
        ↑                                       ↑
   ← 事件通知 ←—— [event_tx] —→  处理结果 —------->
   
用户输入 → [command_tx] → 命令处理 → LLM调用 → 结果推送
```

两个线程需要解耦通信：TUI 需要 `raw_mode` 独占终端，不能阻塞在 LLM HTTP 请求上。用 `mpsc::channel` 是标准的 actor 模式。

重点阅读：`src/main.rs` 第 86-202 行的 `tokio::spawn` 部分，画出完整的数据流图。

### 1.2 Crate 之间的依赖关系

```
core（基础类型，无依赖）
  ↓
llm ──→ core
memory ──→ core
  ↓
agent ──→ core + llm + memory
  ↓
roleplay ──→ core + llm + memory + agent
  ↓
tui ──→ core + llm + memory + roleplay + ratatui + crossterm
```

面试考点：
- 为什么要分成 6 个 crate 而不是一个大 crate？
- 依赖方向是单向的，好处是什么？（编译时间、模块化测试、职责清晰）
- `Cargo.toml` 中 workspace dependencies 的作用

---

## 第二阶段：核心功能实现逻辑（3-4天）

### 2.1 LLM 客户端 —— OpenAI 兼容协议

**重点文件：** `crates/llm/src/client.rs`（276行）

**要实现什么：** 兼容 OpenAI API 的 HTTP 客户端，支持流式和非流式。

**非流式流程：**
```
构建 ChatCompletionRequest (JSON) 
  → POST /chat/completions
  → 解析 ChatCompletionResponse
  → 提取 choices[0].message.content
  → 转为 Message 返回
```

**流式流程（SSE 解析）：**
```
POST 请求 (stream: true)
  → 返回 SSE 格式的数据流：
      data: {"choices":[{"delta":{"content":"你好"}}]}
      data: {"choices":[{"delta":{"content":"世界"}}]}
      data: [DONE]
  
  → 在 spawn 中逐块解析：
      1. 累积 bytes → String buffer
      2. 按 '\n' 分割 SSE 行
      3. 去掉 "data: " 前缀
      4. 解析 JSON → 提取 delta.content
      5. 通过 channel 发送 StreamChunk::Delta
  → 主任务通过 ReceiverStream 消费增量
```

面试考点：
- SSE 协议 vs WebSocket 的区别？（SSE 是单向、HTTP 之上的简单协议）
- 为什么要手动 buffer 解析而不是用 SSE 库？（轻量 + 学习目的）
- `bytes_stream()` 和 `.next().await` 是怎么工作的？

### 2.2 角色 Agent 与上下文管理

**重点文件：** `crates/roleplay/src/character_agent.rs`（60行）

```rust
impl Agent for CharacterAgent {
    async fn run(&self, messages: &[Message]) -> Result<Message> {
        // 1. 构建 messages = [system_prompt(角色人设)] + [windowed_conversation]
        let prepared = self.build_messages(messages);
        
        // 2. 调用 LLM（可能用角色自定义的 model）
        let response = if let Some(model) = self.character.model.as_deref() {
            self.client.chat_completion_with_model(&prepared, model).await?
        } else {
            self.client.chat_completion(&prepared).await?
        };
        
        // 3. 附加角色名，返回
        Ok(response.with_character(&self.character.name))
    }
}
```

**关键设计：**
- 每个角色有自己的 `system_prompt`（人设注入到对话开头）
- 用 `SlidingWindowContext` 截断对话（避免超过 token 上限）
- 支持角色使用不同的 LLM model

### 2.3 Director 模式 —— 用 LLM 编排 LLM

**重点文件：** `crates/roleplay/src/director.rs`（104行）

```
导演 System Prompt:
"你是一个叙事导演，决定哪个角色应该说话..."

输入: 最近10条对话 + 可用角色列表
      → LLM 判断叙事走向
      → 解析返回文本，匹配角色名
      → 返回 [speaker_id1, speaker_id2, ...]
      
Fallback: 如果 LLM 返回的名字不存在 → 选第一个可用角色
```

面试考点：
- "用 AI 编排 AI" 的优缺点？（灵活 vs 不可预测 + 额外 token 成本）
- 如果不用 LLM 做导演，可以怎么做？（规则引擎、优先级队列、随机）

### 2.4 回合管理 —— 三种策略

**重点文件：** `crates/roleplay/src/turn_manager.rs`（77行）

```rust
match &self.strategy {
    TurnStrategy::RoundRobin => 按索引轮转,
    TurnStrategy::DirectorControlled => 调用 Director 的 LLM 判断,
    TurnStrategy::Random => 系统时间种子取模,
}
```

### 2.5 消息流转全过程

从用户输入到 AI 回复的完整链路：

```
1. 用户在 TUI 输入 "你好" → Enter
2. TUI 解析 → Command::SendMessage("你好")
3. 发送到 command_tx channel
4. 命令处理任务收到消息:
   a. 创建 Message::user("你好")，加入 ConversationMemory
   b. 推送到 TUI 显示（event_tx）
   c. 调用 TurnManager.next_speakers() → 返回 [Elena, Theron]
5. 对每个说话者:
   a. 找到对应的 CharacterAgent
   b. agent.run(history) → 调用 LLM API
   c. 收到响应 → 加入 ConversationMemory
   d. 推送到 TUI 显示
6. TUI 收到 NewMessage 事件 → 刷新界面 → 自动滚动到底部
```

---

## 第三阶段：ratatui 框架（2-3天）

### 3.1 ratatui 基本概念

ratatui 是一个 **即时渲染** 的终端 UI 库。和 web 框架不同，每次需要重新绘制整个屏幕。

**核心概念：**
```
Frame（一帧画面）
  ├── Terminal.draw(|f| { render(f) })  —— 每帧调用一次 render 函数
  │
Widget（控件）
  ├── Paragraph（段落/文本）
  ├── List（列表，固定高度条目）
  ├── Block（边框容器）
  ├── Layout（布局，按约束分割区域）
  │
Style（样式）
  ├── Color（前景/背景色）
  ├── Modifier（BOLD、ITALIC 等）
  │
Text（文本内容）
  ├── Text → 多行文本
  ├── Line → 单行，由多个 Span 组成
  ├── Span → 带样式的文本片段
```

**渲染流程（每帧）：**
```
1. event::poll(timeout) —— 检查键盘输入
2. 处理输入，更新 App 状态
3. terminal.draw(|f| { draw(f, &app) }) —— 绘制当前状态
4. 循环回到 1
```

### 3.2 本项目 TUI 布局

**重点文件：** `crates/tui/src/ui.rs`（183行）

```
整个终端窗口
├── Story Bar (高度3行) —— 标题、场景名、消息数
├── 中间区域（最小10行）
│   ├── Chat Panel (75%) —— 对话内容
│   └── Characters Sidebar (25%) —— 角色列表
└── Input Box (高度3行) —— 用户输入框
```

**布局代码：**
```rust
// 垂直分割
let main_chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(3),    // Story bar 固定3行
        Constraint::Min(10),      // 中间区域至少10行
        Constraint::Length(3),    // Input 固定3行
    ])
    .split(f.area());

// 中间区域水平分割
let middle_chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(75),
        Constraint::Percentage(25),
    ])
    .split(main_chunks[1]);
```

### 3.3 Chat 面板渲染（重点理解）

**为什么不用 List？** `List` 的每个 `ListItem` 占一行，不换行。长文本会被截断。

**改用 Paragraph + Wrap：**
```rust
// 把所有消息拼成 Text（多行）
let mut lines: Vec<Line> = Vec::new();
for msg in &app.messages {
    lines.push(Line::from(Span::styled(
        format!("[{}]", msg.character_name),  // 角色名占一行
        header_style,
    )));
    for text_line in msg.content.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {}", text_line),        // 内容缩进
            style,
        )));
    }
    lines.push(Line::from(""));               // 消息间空行
}

// 用 Paragraph 渲染，开启 word wrap
let chat = Paragraph::new(Text::from(lines))
    .wrap(Wrap { trim: false })  // 不换行 trim 保留缩进
    .scroll((scroll, 0))        // 垂直滚动
    .block(Block::default().borders(Borders::ALL));
```

**自动滚动逻辑：**
```rust
let inner_height = area.height.saturating_sub(2); // 去掉边框
let total_lines = text.lines.len();
let scroll = if app.scroll_offset == 0 {
    // 自动滚动：显示最新内容
    total_lines.saturating_sub(inner_height) as u16
} else {
    // 用户 PageUp 后：保持偏移
    total_lines.saturating_sub(inner_height)
        .saturating_sub(app.scroll_offset as usize) as u16
};
```

### 3.4 输入框与光标

**重点文件：** `crates/tui/src/app.rs` 的 `handle_key` 方法

光标定位需要计算 **终端显示宽度**（CJK 字符占 2 格）：
```rust
use unicode_width::UnicodeWidthStr;

let text_before_cursor: String = app.input.chars().take(app.cursor_position).collect();
let display_width = UnicodeWidthStr::width(text_before_cursor.as_str()) as u16;

f.set_cursor_position((area.x + display_width + 1, area.y + 1));
```

### 3.5 TUI 事件循环

**重点文件：** `crates/tui/src/app.rs` 的 `run` 方法

```rust
pub async fn run(mut self, mut event_rx: mpsc::Receiver<AppEvent>, command_tx: mpsc::Sender<Command>) {
    // 1. 进入 raw mode（终端不缓冲、不显示输入字符）
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    
    // 2. 事件循环
    loop {
        terminal.draw(|f| ui::draw(f, &self))?;
        
        // 2a. 键盘输入（50ms 超时，让给异步任务）
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if let Some(cmd) = self.handle_key(key.code, key.modifiers) {
                    let _ = command_tx.send(cmd).await;
                }
            }
        }
        
        // 2b. 处理来自命令处理任务的事件
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::NewMessage(msg) => self.add_message(msg),
                AppEvent::StreamDelta { character, delta } => {
                    // 追加到最新消息（流式打字效果）
                    if let Some(last) = self.messages.last_mut() {
                        if last.character_name == character {
                            last.content.push_str(&delta);
                        }
                    }
                }
                // ...
            }
        }
    }
    
    // 3. 退出时恢复终端
    disable_raw_mode()?;
    execute!(LeaveAlternateScreen, DisableMouseCapture)?;
}
```

**面试考点：**
- `enable_raw_mode()` 做了什么？（终端不再自动换行、不回显）
- 为什么 `poll` 要有 50ms 超时？（给 `event_rx.try_recv()` 机会处理异步消息）
- 退出时为什么要 `disable_raw_mode()` 和 `LeaveAlternateScreen`？

---

## 第四阶段：工程问题与解决方案（1-2天）

### 4.1 UTF-8 字符索引问题

**问题：** 中文输入 panic。`String::insert(byte_idx, char)` 要求 byte 是 char 边界。

**原因：** Rust String 是 UTF-8，中文字符 3 字节。用 char index 当 byte index 会插入到字节中间。

**修复：** 见 `crates/tui/src/app.rs` 的 `char_to_byte_index()` 和 `docs/bugfix-chinese-input-panic.md`

### 4.2 文本换行问题

**问题：** 终端窄时 chat 内容被截断。

**原因：** `List` 不换行，每个 `ListItem` 强制一行。

**修复：** 改用 `Paragraph + Wrap`。见 `docs/bugfix-chat-text-wrapping.md`

### 4.3 Context Window 管理

**重点文件：** `crates/memory/src/sliding_window.rs`

```
目标: 让发给 LLM 的消息不超过 token 上限
策略:
  1. 永远保留 system messages（人设不能丢）
  2. 取最近 N 条非 system 消息
  3. 如果总字符数超限，从旧消息开始删
```

面试考点：
- 字符数 ≈ token 数？不准确，但够用了
- 更好的方案：实际 token 计数 + 对话摘要

---

## 第五阶段：简历包装与面试准备（1-2天）

### 简历描述建议

```
项目：Rust Multi-AI Roleplay Agent（个人项目）
- 使用 Rust 实现多 AI 角色扮演聊天系统，Cargo workspace 多 crate 架构，6个 crate 分层
- 实现 LangGraph 风格状态图引擎（StateGraph），支持节点、条件边、动态路由
- 实现 OpenAI 兼容 LLM 客户端，手动解析 SSE 流式响应，通过 tokio channel 推送增量
- 使用 ratatui + crossterm 构建 TUI，解决 UTF-8 索引和 CJK 字符宽度问题
- 设计 Director 模式用 LLM 控制叙事流程，实现多角色自动对话编排（RoundRobin/Director/Random策略）
- 实现 SlidingWindow 上下文管理，在 token 限制下保留系统提示和最近对话
技术栈：Rust, tokio, async/await, ratatui, crossterm, reqwest, serde, thiserror, tokio-stream
```

### 高频面试问题

| 问题 | 回答要点 |
|------|----------|
| 项目架构？ | 6层：core→llm→memory→agent→roleplay→tui，单向依赖 |
| 为什么多 crate？ | 编译时间、职责清晰、模块可独立测试 |
| 异步架构？ | TUI主线程 + spawn命令处理，两对channel通信 |
| 流式响应？ | SSE解析，spawn逐块解析→channel→TUI逐字显示 |
| 角色怎么"说话"？ | 每个角色一个Agent，拼system_prompt+对话→LLM→回复 |
| Director模式？ | 用LLM决定谁下一个说话，fallback到轮转策略 |
| ratatui怎么用？ | Layout分割→Widget渲染→每帧draw→raw mode |
| 遇到过什么坑？ | UTF-8 byte/char index混淆、List不换行 |
| Context怎么管理？ | SlidingWindow，保system，取最近N条，超限从旧删 |

---

## 后续扩展（加分项）

| 功能 | 难度 | 面试亮点 |
|------|------|----------|
| 单元测试 | ⭐ | 测试意识 |
| tiktoken token计数 | ⭐⭐ | 精确上下文管理 |
| Tool Use（搜索/文件） | ⭐⭐ | Agent 能力扩展 |
| 多Provider（Anthropic） | ⭐⭐ | 架构抽象能力 |
| /save /load 完整实现 | ⭐ | 持久化设计 |
| 流式打字效果 | ⭐ | 用户体验 |
