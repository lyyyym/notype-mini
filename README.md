# NoType Mini

一款轻量级 macOS 桌面语音转写工具：按住快捷键说话，松开后自动把口语整理成规范文字并输入到当前光标处。

> 受 Typeless / NoType 启发，作为个人练手项目，MVP 版本聚焦「按住说话 → AI 整理 → 自动输入」。

## ✨ 核心特性

- **两种录音模式**：
  - **按住说话**：按住 `⌘ + .` 录音，松开自动处理。
  - **连续录音**：按一次 `⌘ + .` 开始，再按一次结束，录音中按 `Esc` 取消。
- **AI 口语整理**：自动去除「嗯、啊、那个」等填充词，修正改口，自动分段加标点。
- **系统级文字注入**：通过模拟键盘把结果直接打到光标所在位置，支持任意应用。
- **智能输入回退**：长文本或模拟键盘失败时自动改用剪贴板粘贴，提高稳定性。
- **系统托盘常驻**：关闭主窗口后应用仍在托盘运行，右键可显示/隐藏窗口或退出。
- **双输出模式**：智能整理 / 逐字转写，随时切换。
- **本地历史与统计**：保存最近 200 条转写，支持复制、删除、导出 Markdown，统计累计/今日字数。
- **轻量设置界面**：配置 API Key、模型、录音模式、自动回车、剪贴板阈值等。

## 🛠 技术栈

| 模块 | 方案 |
|---|---|
| 桌面框架 | Tauri v2（Rust 后端 + React/TypeScript 前端） |
| 音频采集 | Rust `cpal` |
| 键盘模拟 | Rust `enigo` |
| 语音识别 | 阿里云 DashScope（Paraformer / Qwen-ASR） |
| 文本润色 | OpenAI 兼容 API（DeepSeek / Moonshot 等） |
| 配置/历史 | 本地 TOML / JSON，存储于 `~/Library/Application Support/notype-mini/` |

## 📦 安装与运行

### 环境要求

- macOS 11+
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install)（通过 rustup）

### 本地开发

```bash
cd notype-mini
npm install
npm run tauri dev
```

首次运行会提示授权：

1. **麦克风权限**：用于录音。
2. **辅助功能权限**：用于向其他应用注入文字。

## ⚙️ 配置

启动后进入设置界面，填写以下信息：

### DashScope 语音识别

| 字段 | 说明 |
|---|---|
| Base URL | `https://dashscope.aliyuncs.com` 或你的工作空间 URL |
| API Key | 阿里云百炼 API Key |
| Model | `qwen3-asr-flash` 或 `paraformer-v2` |

> 推荐 `qwen3-asr-flash`，对中文口语识别效果较好。

### LLM 润色

| 字段 | 说明 |
|---|---|
| Base URL | `https://api.deepseek.com` 或其他 OpenAI 兼容端点 |
| API Key | 对应服务商 API Key |
| Model | `deepseek-chat` 等 |

## 🚀 使用方式

1. 把光标放在任意输入框（备忘录、微信、VS Code、浏览器等）。
2. 选择录音模式：
   - **按住说话**：按住 `⌘ + .`，屏幕中央会出现录音气泡；松开自动处理。
   - **连续录音**：按一次 `⌘ + .` 开始录音，再按一次结束；录音中按 `Esc` 取消。
3. 说话，松开/结束录音后 1~3 秒内文字会自动出现在光标处。
4. 打开主窗口可查看历史记录、统计和设置。

## ⌨️ 快捷键

| 快捷键 | 功能 |
|---|---|
| `⌘ + .` | 按住说话模式下：按住录音，松开结束；连续录音模式下：按一次开始，再按结束 |
| `Esc` | 取消当前录音 |

## 📁 数据存储

- 配置文件：`~/Library/Application Support/notype-mini/config.toml`
- 历史记录：`~/Library/Application Support/notype-mini/history.json`

## 📝 版本规划

- [x] MVP：按住说话 → AI 整理 → 光标输入 → 历史记录
- [x] V2：连续录音模式、系统托盘常驻、剪贴板回退
- [ ] V3：语音编辑、多引擎切换、个人词典

## 📄 许可证

MIT

## 🙏 致谢

- 受 [NoType](https://github.com/NoType) / Typeless 启发
- Tauri、cpal、enigo、DashScope、DeepSeek 等开源项目与 API 服务
