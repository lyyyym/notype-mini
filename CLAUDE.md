# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

NoType Mini 是一款基于 Tauri v2 的 macOS 桌面语音转写应用。用户通过全局快捷键录音，后端将音频发送到 DashScope 进行语音识别，再经 OpenAI 兼容 LLM 润色或按指令编辑，最后通过模拟键盘把文字注入当前应用光标处。V3 新增个人词典和语音编辑功能。

## 常用命令

所有命令都在仓库根目录执行。

```bash
# 安装前端依赖
npm install

# 本地开发（启动 Vite + Tauri，自动打开主窗口）
npm run tauri dev

# 仅类型检查前端（不生成产物）
npx tsc --noEmit

# 构建前端产物（用于 tauri build）
npm run build

# 运行全部 Rust 测试
cd src-tauri && cargo test

# 运行单个测试模块
cd src-tauri && cargo test --lib dictionary::tests

# 运行单个测试
cd src-tauri && cargo test --lib dictionary::tests::test_longest_match

# Rust 快速检查（不运行测试）
cd src-tauri && cargo check
```

首次运行 `npm run tauri dev` 时，macOS 会提示授权**麦克风**和**辅助功能**权限，必须授予才能录音和向其他应用注入文字。

## 技术栈

- 桌面框架：Tauri v2（Rust 后端 + React/TypeScript 前端）
- 音频采集：Rust `cpal`
- 键盘模拟：Rust `enigo`
- 剪贴板：`arboard`
- 语音识别：阿里云 DashScope（Paraformer / Qwen-ASR）
- 文本润色/编辑：OpenAI 兼容 API（DeepSeek 等）
- 配置/历史：本地 TOML / JSON，存储于 `~/Library/Application Support/notype-mini/`

## 代码架构

### 窗口与前端路由

`src-tauri/tauri.conf.json` 配置了两个窗口：

- `main`：主设置/历史窗口（700×500）。
- `bubble`：录音时的圆形悬浮气泡（220×220，无边框、透明、置顶）。

`src/main.tsx` 根据 `getCurrentWebviewWindow().label` 决定渲染 `<App />` 还是 `<Bubble />`。

### 后端核心状态

`src-tauri/src/lib.rs` 中的 `AppState` 是单一事实来源，包含：

- `config: Mutex<Config>`：运行时配置。
- `session: Mutex<SessionState>`：当前活跃模式（Idle / Transcribe / Edit）和录音句柄。
- `parsed_shortcut` / `parsed_edit_shortcut`：已解析的全局快捷键对象。

所有 Tauri 命令都通过 `State<AppState>` 访问这些状态。

### 全局快捷键

- `⌘+.`：录音快捷键，行为取决于 `recording_mode`（`push_to_talk` 按住说话 / `continuous` 连续录音）。
- `⌘+Option+.`：语音编辑快捷键（默认），先选中文本后按住说话，松开后替换选区。
- `Esc`：取消当前录音或编辑。

快捷键字符串通过 `lib.rs` 中的 `parse_shortcut()` 解析为 `tauri-plugin-global-shortcut::Shortcut`。`set_config` 会**只注册发生变化的快捷键**，成功后再注销旧快捷键；如果新旧快捷键相同，则直接保存配置，避免重复注册。

### 录音流程

1. `audio::start_recording()` 在独立线程中创建 `cpal` 输入流，返回 `RecorderHandle`。
2. 快捷键释放后调用 `RecorderHandle::stop()`，得到 `(Vec<i16>, sample_rate)`。
3. `audio::samples_to_wav()` 把 PCM 编码为 WAV。
4. 根据当前模式进入 `transcribe_pipeline()` 或 `edit_pipeline()`。

### 转写流程（transcribe_pipeline）

1. `dashscope::transcribe()` 识别音频。
2. `dictionary::apply_dictionary()` 应用个人词典替换。
3. 如果 `output_mode == "polish"`，调用 `llm::polish()` 润色；否则保留原文。
4. `type_text()` 通过 `enigo` 模拟键盘输入；字数超过 `clipboard_fallback_threshold` 或键盘输入失败时回退到 `Cmd+V` 剪贴板粘贴。
5. 保存到 `history::add_entry()`，类型为 `EntryType::Transcribe`。

### 语音编辑流程（edit_pipeline）

1. `dashscope::transcribe()` 识别指令文本。
2. `capture_selected_text()` 通过 `Cmd+C` 捕获当前选中文本：先把一个 sentinel 写入剪贴板，再模拟 `Cmd+C`，最后比较剪贴板内容变化。
3. 调用 `llm::edit(instruction, selected_text)` 生成结果。
4. 删除原选区，把结果输入到光标处。
5. 恢复原始剪贴板内容。
6. 保存到历史记录，类型为 `EntryType::Edit`。

### 个人词典

词典条目为 `DictionaryEntry { from, to }`，存储在 `Config.dictionary` 中。替换算法位于 `src/dictionary.rs`：

- 从文本开头扫描到结尾。
- 每个位置尝试所有 `from`，选择匹配的最长者。
- 替换后光标前进 `from` 长度，不重复匹配已替换部分，避免级联替换。
- ASCII 字母不区分大小写；中文按字符边界匹配。

`validate_dictionary()` 在 `set_config` 中校验：跳过 `from` 为空的条目，拒绝 `from` 重复。

### 配置与历史持久化

- 配置：`src-tauri/src/config.rs`，路径 `~/Library/Application Support/notype-mini/config.toml`。
- 历史：`src-tauri/src/history.rs`，路径 `~/Library/Application Support/notype-mini/history.json`，最多保存 200 条。
- 新字段必须加 `#[serde(default)]` 或自定义 default 函数，保证旧配置能解析。

### 重要约束

- `bubble` 窗口显示时**不能调用 `set_focus`**，否则会抢走当前应用焦点导致光标丢失。
- 关闭主窗口时调用 `api.prevent_close()` 并隐藏窗口，应用通过系统托盘常驻。
- `set_config` 拒绝在录音/编辑活跃时修改配置。
- 录音快捷键和编辑快捷键不能相同，且都不能是裸 `Esc`。

## 版本号同步

升级版本时，需要同时修改：

- `package.json` 的 `version`
- `src-tauri/Cargo.toml` 的 `version`
- `src-tauri/tauri.conf.json` 的 `version`

## 快捷键字符串格式

配置中快捷键使用 `+` 连接，例如：

- `Command+Period`
- `Command+Option+Period`
- `Control+Shift+Space`

支持的修饰键：`Command`/`Cmd`/`Super`、`Control`/`Ctrl`、`Option`/`Alt`、`Shift`。支持的按键包括字母、数字、标点符号以及 `Escape`、`Space`、`Return`、`Tab` 等（完整映射见 `lib.rs` 的 `normalize_key()`）。

## 已踩过的坑

### 全局快捷键重复注册

macOS 下 `tauri-plugin-global-shortcut` 不允许重复注册同一个热键。`set_config` 早期实现会在每次保存配置时重新注册当前快捷键，即使用户没有修改快捷键，这会导致 `RegisterEventHotKey failed for Period` 错误并让整个保存失败。

**正确做法：** 在 `set_config` 中先比较新旧快捷键是否相同，相同则跳过注册；只注册真正变化的快捷键，注册成功后再注销旧快捷键。失败时必须回滚已注册的新快捷键。
