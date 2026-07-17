# Plan Review Log: 从纠正历史自动学习词典词条

Act 1 (grill) complete — plan locked with the user. MAX_ROUNDS=5.

## Round 1 — Codex

VERDICT: REVISE

主要问题与修订：
1. **diff 原始端应为 raw_text 而非 polished_text**：候选 from 必须命中未来 ASR 输出，因此后端 submit_correction 要用同一历史条目的 raw_text 作为 diff 基准。已在 PLAN.md 中明确。
2. **similar crate 不会自动合并相邻 delete/insert**：计划原本只说"归并"，未写具体算法。已补充显式 coalesce 逻辑说明。
3. **CandidateStore 字段需 serde(default)、count 用 u32**：已采纳写入计划。
4. **submit_correction 需长度上限、日志、错误事件**：已补充到计划。

未采纳：
- 文件锁/审计日志/ rejected_at：单实例应用，认为过度设计。
- 把 candidates 放进 config.toml：违反用户确立的手动保存语义。
- accept_candidate 直接保存 config：同上，违反手动保存。
- word-level diff / jieba 分词：字符级更贴合现有 dictionary.rs 行为，且单字纠错有价值。

### Claude's response
已按上述修订更新 PLAN.md，准备进入 Round 2。
