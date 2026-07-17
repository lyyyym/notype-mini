# Plan: V3.1 — 升级 enigo 到 0.3 并验证焦点/输入稳定性

_Locked via grill — by Claude + yiming_

## Goal

将 NoType Mini 的键盘模拟依赖从 enigo 0.2.1 升级到 0.3.0，验证升级后 macOS 上的焦点保持、文字输入、剪贴板粘贴稳定性，并在验证成功的前提下尽量简化当前为 0.2.1 bug 引入的 workaround（`Key::Other` 模拟 `Cmd+C/V`）。V3.1 不改动强制剪贴板回退策略和气泡窗口焦点修复，只作为一次小版本升级 + 稳定性验证。

## Approach

1. **依赖升级**
   - `src-tauri/Cargo.toml`：`enigo = "0.2"` → `enigo = "0.3"`（锁定 0.3，不跟随 0.6）。
   - `cargo check` 和 `cargo test` 确认编译与单元测试通过。

2. **尝试简化 workaround**
   - 将 `capture_selected_text` 中的 `Cmd+C` 从 `Key::Other(8)` 改回 `Key::Unicode('c')`。
   - 将 `type_text` 中的 `Cmd+V` 从 `Key::Other(9)` 改回 `Key::Unicode('v')`。
   - 保留 `force_clipboard_fallback` 参数和强制剪贴板回退逻辑；本次不恢复 `enigo.text()` 短文本路径。

3. **版本号同步**
   - `package.json`：`0.3.0` → `0.3.1`
   - `src-tauri/Cargo.toml`：`0.3.0` → `0.3.1`
   - `src-tauri/tauri.conf.json`：`0.1.0` → `0.3.1`

4. **验证（按 TEST.md 执行）**
   - P0 必过：
     - 普通录音转写（⌘+.）文字插入原光标处，不丢到 NoType Mini 自己。
     - 语音编辑（⌘+Option+.）替换选中文字在原应用内。
     - 口述 200+ 字长文本不崩溃、能粘贴成功。
     - 连续按快捷键不会并发输入/崩溃。
   - P1：
     - 非 QWERTY 键盘布局下 `Cmd+C` / `Cmd+V` 正确。
     - 剪贴板在输入后 1 秒内恢复为用户原来的内容。

5. **回滚策略**
   - 如果任何 P0 用例失败，立即 `git revert` 到 enigo 0.2 + 当前 workaround 状态。
   - 不删除原 `Key::Other` 代码，初次提交改为 `Key::Unicode`，便于 revert。

6. **文档更新**
   - 更新 `CLAUDE.md`：
     - 将 enigo 版本说明改为 0.3。
     - 若 `Key::Unicode` 验证通过，将"已踩过的坑#3"标记为历史问题（0.3 已修复）或移除。
     - 保留强制剪贴板回退的说明。
   - 更新 `TEST.md`：
     - 新增用例：enigo 0.3 升级后长文本（>100 字）不崩溃。
     - 新增用例：验证剪贴板在输入后 1 秒内恢复。

7. **INSERTION_FIXME.md 处理**
   - 不提交该文件到仓库。
   - 将其中的 14 条缺陷拆解为 GitHub issues，然后在本地删除该文件。

## Key decisions & tradeoffs

- **锁定 enigo 0.3 而非 0.6**：0.3 只升级 enigo 自身及少量依赖，风险可控；0.6 会连带升级 Tauri、arboard、global-hotkey 等核心依赖，超出 V3.1 "小步验证" 的范围。
- **保留强制剪贴板回退**：该策略解决的不只是 enigo 0.2.1 的崩溃，还包括 macOS 上 `enigo.text()` 事件被丢弃导致文字插不进光标的问题。V3.1 先不动它，等 0.3 稳定后再评估是否恢复短文本键盘输入路径。
- **先改回 `Key::Unicode`，失败再 revert**：这是验证 0.3 是否真正修复了 macOS 栈溢出的最直接方式。如果失败，回滚成本最低。
- **不新增自动化测试**：macOS GUI/焦点/剪贴板行为难以单元测试，继续以 TEST.md 手工用例为主，避免 V3.1 范围膨胀。

## Risks / open questions

- enigo 0.3 的 macOS 实现改动可能引入新的输入时序问题（如 `Cmd+C` 发送后系统未及时处理）。
- 0.3 新增的辅助功能权限请求逻辑可能与 Tauri 的权限流程冲突，首次运行行为需验证。
- 非 QWERTY 布局下 `Key::Unicode('c')` / `Key::Unicode('v')` 是否仍能正确触发 `Cmd+C` / `Cmd+V`，需要实际测试。

## Out of scope

- 不升级 enigo 到 0.4/0.5/0.6。
- 不恢复 `enigo.text()` 短文本输入路径。
- 不解决 INSERTION_FIXME.md 中的并发 pipeline、Esc 全局热键、剪贴板陈旧快照等深层架构问题（这些拆分为独立 GitHub issues，后续版本处理）。
- 不重构 `type_text` 以外的焦点管理代码。
