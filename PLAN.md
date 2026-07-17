# Plan: 从纠正历史自动学习词典词条

_Locked via grill — by Claude + 用户_

## Goal

目前个人词典完全手动维护，用户容易忘记保存、也不知道该加哪些词。本功能在历史记录中为"录音"类型条目提供「修正」入口：用户把识别结果改成正确版本后保存，应用自动 diff **ASR 识别原文（raw_text）** 与 **用户修正版本** 得到 `错误词 → 正确词` 候选词条，进入词典卡片里的"待确认"队列；用户一键接受后词条进入正式词典（走既有手动保存流程），一键拒绝则永久拉黑。以此把词典维护从"纯手工录入"变成"纠正即学习、确认即生效"。

## Approach

1. **新建 `src-tauri/src/candidates.rs` 模块**
   - `CandidateStore { pending: Vec<CandidateEntry>, rejected: Vec<RejectedPair> }`，两个字段都加 `#[serde(default)]`，保证后续扩展向后兼容。
   - `count` 用 `u32` 而非 `usize`，跨平台序列化一致。
   - 持久化到 `~/Library/Application Support/notype-mini/candidates.json`（与 history.json 同目录，独立于 config.toml）。
   - `add_candidates(pairs, existing_dictionary)`：
     - 跳过 `from` trim 后为空的片段（纯新增文本无法做词典 `from`）。
     - 跳过 `from == to` 的无意义对。
     - 跳过已在现有词典中的 `(from, to)`（ASCII 不区分大小写比较，与 dictionary.rs 匹配语义一致）。
     - 跳过已在 `rejected` 黑名单中的对。
     - 已在 pending 中的对：`count += 1`；否则新增 `count = 1`。
   - `accept(from, to)`：从 pending 移除并返回该对。
   - `reject(from, to)`：从 pending 移除并加入 rejected（永久不再出现）。
   - 单元测试覆盖以上全部规则。

2. **diff 提取（放在 candidates.rs 或独立函数，用 `similar` crate）**
   - `extract_correction_pairs(original: &str, corrected: &str) -> Vec<(String, String)>`
   - **注意：diff 的 `original` 必须是 `raw_text`（ASR 识别原文，而非 polished_text）**，这样候选 `from` 才能命中未来 ASR 输出。用户在前端编辑的是 `polished_text`，但后端 submit_correction 会把同一 id 下的 `raw_text` 取出来做 diff。
   - 字符粒度 diff（`similar::TextDiff`，`iter_all_changes` 按 tag 分组）。**关键实现细节**：`iter_all_changes()` 不会自动把相邻的 Delete + Insert 合并为一次替换，必须显式 coalesce：顺序遍历 changes，当遇到 Delete 时暂存其文本；若下一条是 Insert 则合并为 `(deleted, inserted)`；若 Delete 后紧跟 Equal 或文件结束，则作为 `to=""` 输出；纯 Insert（from 为空）跳过。
   - 中文按 char 切分（与 dictionary.rs 的 char 语义一致）。emoji/组合字符可能按 char 被切分，与现有词典行为一致，文档中说明限制。
   - Cargo.toml 增加 `similar = "2"` 依赖。
   - 单元测试：单处替换、多处替换、纯删除、纯新增（无候选）、改标点。

3. **history.rs 增加 `update_polished_text(id, new_text)`**：按 id 找到条目，更新 `polished_text` 和 `word_count`，保存。修正只更新历史记录，**不**重新注入目标应用（光标上下文已不存在）。

4. **lib.rs 注册模块并新增 4 个 Tauri 命令**
   - `submit_correction(id, corrected_text) -> Result<Vec<CandidateEntry>, String>`：
     - 校验 `corrected_text` 非空、长度 ≤ 10000 字符、与当前 `polished_text` 不同。
     - 按 id 取出历史条目，用 `raw_text` 作为 `original`、`corrected_text` 作为 `corrected` 提取候选对。
     - 调用 `add_candidates`（传入当前 `config.dictionary` 的快照）。
     - 更新历史条目 `polished_text` 和 `word_count`。
     - 每个关键步骤打 `eprintln!` 日志；失败时通过 `emit_error` 向前端发送错误事件。
     - 返回最新 pending 列表。
   - `get_candidates() -> Vec<CandidateEntry>`。
   - `accept_candidate(from, to) -> Result<(), String>`：只动 candidates.json，不动 config（前端负责把词条加进本地 config 状态）。
   - `reject_candidate(from, to) -> Result<(), String>`。
   - 这些命令在录音/编辑活跃时不拒绝（与 set_config 不同，它们不触碰快捷键和会话状态）。

5. **前端 `src/App.tsx`**
   - 历史记录卡片（仅 `entry_type === "transcribe"`）：操作区增加「修正」按钮。点击后 `polished_text` 区域变为 `<textarea>`（默认填入当前 polished_text）+「保存修正」「取消」。保存时调用 `submit_correction`，成功后刷新历史与候选队列，状态栏提示"已修正，N 条候选进入待确认"。
   - 词典卡片新增"待确认候选"区（在正式词条列表下方、添加输入行上方）：每条显示 `from → to`、`×N` 次数徽标、「接受」「拒绝」按钮。
     - 接受：`accept_candidate` → 词条追加到 `config.dictionary` → `dictDirty = true`（复用未保存警告条，用户点「立即保存」后生效）→ 刷新候选。
     - 拒绝：`reject_candidate` → 刷新候选。
   - 挂载时 `get_candidates()` 加载一次。
   - 样式沿用现有 card/btn 体系，候选区视觉弱化（灰底、小字），与正式词条区分。

6. **验证**
   - `cargo test` 全绿（新增 candidates/diff 测试 + 既有测试）。
   - `npx tsc --noEmit` 通过。
   - `npm run tauri dev` 手动验证：录音一句 → 历史中点「修正」改一词 → 候选队列出现 → 接受 → 警告条出现 → 立即保存 → 再次录音验证词典命中 → 拒绝另一条候选 → 重复同样修正验证不再出现。

7. **版本号**：按 CLAUDE.md 约定三处同步升级（package.json / Cargo.toml / tauri.conf.json），0.3.0 → 0.4.0（tauri.conf.json 当前滞留 0.1.0，一并同步）。

## Key decisions & tradeoffs

- **纠正数据来源是"用户手动修正历史"，不是 raw vs polished 的自动 diff**：后者学到的是 LLM 润色习惯（删"嗯"、改口修正），噪声极大；前者才是真实用户纠正。代价是需要新 UI 入口，且只能学用户愿意动手改的记录。
- **候选 `from` 来自 `raw_text`（ASR 识别原文）而非 `polished_text`**：用户在前端编辑的是润色后文本，但为了能真正命中未来 ASR 输出，diff 的原始端使用 `raw_text`。这可能导致候选和用户在编辑框里看到的修改不完全一致（例如 LLM 把"北京大学"润色成"北大"，用户改回"北京大学"，实际学到的是 `北大 → 北京大学` 或 `北京大学 → 北京大学`，取决于 raw_text），靠人工确认环节兜底。
- **diff 用 `similar` crate 字符级 diff，需显式 coalesce**：`iter_all_changes()` 不会自动合并相邻 delete+insert，计划已要求显式实现合并逻辑。字符级在中文里可能产生单字候选（如"的→地"），有时正是用户想要的，不引入 jieba 分词依赖。
- **待确认队列 + 人工接受，无频次门槛**：确认环节本身已是过滤器，频次门槛只延迟可见性、不去噪。代价是一次性笔误也会出现在队列里（拒绝成本一键）。
- **拒绝即永久拉黑**：避免同一坏候选反复骚扰；拒绝错了只能手动添加（入口已存在）。
- **接受后走既有手动保存流程（dictDirty + 立即保存）**：与用户确立的"手动保存"习惯一致，不引入"接受即生效"的第二种保存语义。
- **diff 用 `similar` crate 而非手写 LCS**：成熟库处理分组边界更可靠，减少自维护代码。代价是新增一个依赖。
- **只对 transcribe 类型开放修正**：edit 记录的 polished_text 是 LLM 生成物而非 ASR 输出，提取的候选无法命中未来识别原文，只会污染队列。

## Risks / open questions

- 多用户并发不存在（单机单实例），但 candidates.json 与 history.json 一样采用"读-改-写"，与录音 pipeline 同时写入的窗口期理论上可能丢一条候选——可接受（下次修正会重新计数）。
- `similar` 的字符 diff 在极端长文本（>1000 字）下的性能未验证；历史记录实际长度通常在几百字内，风险低。

## Out of scope

- 从 raw_text vs polished_text 自动学习（LLM 习惯，噪声大）。
- 监听目标应用中用户的事后修改（技术上不可行/侵入性过强）。
- 修正后把文本重新注入目标应用（光标上下文已丢失）。
- 编辑模式历史记录的修正入口。
- 词典自动保存（用户明确选择手动保存）。
