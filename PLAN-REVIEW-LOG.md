# Plan Review Log: NoType Mini V3 — Personal Dictionary + Voice Editing

Act 1 (grill) complete — plan locked with the user. MAX_ROUNDS=5.

## Round 1 — Codex

Here are the concrete flaws I found in the plan, each with a one-line fix.

**1. Existing config will be wiped on upgrade**
Adding `dictionary` and `edit_shortcut` to `Config` without `#[serde(default)]` breaks deserialization of existing `config.toml`, and `Config::load()` falls back to default values (losing API keys/settings).
Fix: add `#[serde(default)]` to both new fields and add a `config_version` field for future migrations.

**2. TOML section name is wrong**
A `Vec<DictionaryEntry>` serializes as an array-of-tables (`[[dictionary]]`), not a single table (`[dictionary]`).
Fix: document and test the config section as `[[dictionary]]` not `[dictionary]`.

**3. New edit shortcut cannot be dispatched**
`lib.rs` hardcodes `shortcut.key == Code::Period && shortcut.mods == Modifiers::SUPER`, so `Command+Shift+Period` will be ignored even after registration.
Fix: parse both `shortcut` and `edit_shortcut` config strings into Tauri `Shortcut` structs and compare key + full modifiers in the handler.

**4. No mutual exclusion between normal and edit recording**
The plan reuses the single `recorder` field for both modes; pressing both shortcuts in sequence can overwrite the active recording handle.
Fix: add an `ActiveMode` enum (`Idle`/`Transcribe`/`Edit`) and reject the second shortcut while already active.

**5. Missing LLM edit function**
The codebase only exposes `llm::polish()`; the new edit flow needs a matching `edit(instruction, selected_text)` function.
Fix: add `llm::edit` (or a generic `chat_complete`) with the documented system prompt.

**6. Dictionary replacement will cascade and replace substrings**
Sequential `str::replace` can replace substrings ("拉斯特级" → "Rust级") and produce cascading matches from earlier replacements.
Fix: iterate over the text once, replacing only exact non-overlapping matches with whole-word boundaries where appropriate.

**7. Clipboard-based selection is racy and non-text unsafe**
Sleeping after `Cmd+C` and then reading text can miss slow apps, and restoring text-only will corrupt images/files on the original clipboard.
Fix: poll the clipboard for up to ~500ms until text appears, and skip save/restore when the original clipboard is not text.

**8. Selected text may not be replaced reliably**
The result is typed at the cursor, but the prior `Cmd+C` may not keep the selection active; typing then appends rather than replaces.
Fix: after copying selection, simulate `Cmd+X` (or `Cmd+C` then Delete) to ensure the selected text is removed before typing the result.

**9. Prompt injection via selected text**
Arbitrary selected text is concatenated into the LLM user message, so content like "忽略以上指令" can override the system instruction.
Fix: send the instruction in one message and the selected text in a separate message with a delimiter, instructing the model to treat it as data.

**10. Edit history will pollute transcription stats**
Storing edits as ordinary `HistoryEntry` with a synthetic `raw_text` mixes workflows and inflates word/session counts.
Fix: add an `entry_type` enum to `HistoryEntry` and update stats/export to distinguish `Transcribe` from `Edit`.

**11. No validation for edit_shortcut**
The shortcut is stored as a free-form string but never parsed/validated; a typo like "Command+Shift+Dot" will silently fail at runtime.
Fix: validate `edit_shortcut` in `set_config` by parsing it to `Modifiers` + `Code` and return an error if unsupported.

**12. `auto_enter` will wrongly apply to edits**
`type_text` uses the global `auto_enter` flag, so edited text may get an unwanted trailing newline.
Fix: pass `auto_enter: false` when typing edit results.

**13. Simpler alternative for dictionary storage**
Putting dictionary entries inside `config.toml` couples user data with API credentials and makes every dictionary save rewrite the whole config.
Fix: consider storing dictionary in a separate `dictionary.toml` loaded alongside the main config.

VERDICT: REVISE

### Claude's response

Accepted and incorporated all findings except #13 (separate `dictionary.toml`), which was logged as a future improvement to keep V3 simple and consistent with existing config storage. Revisions added to PLAN.md:

- Added `#[serde(default)]` and `config_version` for backward compatibility.
- Documented `[[dictionary]]` TOML array-of-tables format.
- Added parsing/validation of `shortcut` and `edit_shortcut` strings into Tauri `Shortcut`.
- Introduced `ActiveMode` enum to prevent overlapping recording/editing.
- Added `llm::edit` with separate instruction/selected-text messages.
- Replaced naive `str::replace` with non-overlapping single-pass replacement.
- Added clipboard polling and non-text clipboard handling.
- Switched to `Cmd+X`/Delete to ensure selected text is removed before typing result.
- Added `EntryType` to `HistoryEntry` to distinguish Transcribe vs Edit.
- Explicitly pass `auto_enter: false` for edit results.

## Round 2 — Codex

The Round 1 findings are reflected in the revised plan. The remaining issues are new concrete flaws.

**1. No way to distinguish a real selection from stale clipboard text**
The plan saves the original clipboard and polls after `Cmd+C`, but if the user has no selection and the clipboard already contains text, the app will treat that stale text as the selection and edit it.
Fix: set a sentinel string on the clipboard before `Cmd+C`, and treat the post-copy clipboard as a selection only if it differs from the sentinel.

**2. Selected text is deleted before the instruction is even spoken**
The plan states that `Cmd+C` cancels the selection (it does not) and therefore cuts the selection before the user finishes the instruction; if the user cancels or the pipeline fails, the original selection is already gone.
Fix: capture the selection on shortcut release, keep it intact during recording, and delete it only after the LLM result is ready and immediately before typing.

**3. Clipboard capture races with the held shortcut keys**
The plan simulates `Cmd+C` while the user is still pressing `Cmd+Shift+Period`, which can produce key-state confusion and an unreliable copy.
Fix: perform the `Cmd+C` capture after the shortcut has been released and the modifier keys are up.

**4. Active-mode check is not atomic**
The plan says to ignore the second shortcut when already active, but checking and updating the mode in separate mutex locks lets two near-simultaneous shortcuts both pass the idle check.
Fix: hold the state lock for both the idle check and the transition to active in one critical section.

**5. Edit results never refresh the main window**
The main UI refreshes history and stats on `transcription-result`; the plan only updates the bubble for edit mode, so edit history will stay stale until the user manually reloads.
Fix: emit `transcription-result` (or a new `edit-result`) after saving an edit entry.

**6. `config_version` lacks `#[serde(default)]`**
The plan adds `config_version: u32` for future migrations but does not explicitly mark it with `#[serde(default)]`, so an existing config without it will fail to load.
Fix: add `#[serde(default)]` to `config_version`.

**7. `HistoryEntry.entry_type` breaks old history files**
The plan adds `entry_type` to `HistoryEntry` without `#[serde(default)]`, so old `history.json` entries will fail to deserialize and be silently dropped.
Fix: add `#[serde(default)]` to `entry_type` and default existing entries to `Transcribe`.

**8. Shortcut changes in settings do not take effect at runtime**
The plan validates and parses shortcuts in `set_config` and registers them in `run()`, but never re-registers the global OS shortcuts when the user changes them.
Fix: make `set_config` accept an `AppHandle` and re-register the global shortcuts after saving valid config.

**9. The two shortcuts could be configured to the same binding**
The plan validates each shortcut individually but does not prevent `shortcut` and `edit_shortcut` from being identical, making the two modes indistinguishable.
Fix: reject the config when the two shortcut strings are equal.

**10. Dictionary is not applied to edit-mode instructions**
The plan applies dictionary replacement before transcription polish, but does not apply it to the ASR'd instruction in voice-edit mode, so misrecognized command words won't be corrected.
Fix: run `apply_dictionary` on the ASR instruction text before calling `llm::edit`.

**11. Clipboard is not restored on ASR/LLM/type failures**
The plan only mentions restoring the clipboard when the instruction cannot be handled; ASR, LLM, or typing failures will leave the original clipboard replaced by the selected text or an error result.
Fix: wrap the edit pipeline in a cleanup closure that always restores the saved clipboard before returning.

**12. Frontend TypeScript interfaces are not updated**
The plan adds `dictionary`, `edit_shortcut`, `config_version`, and `entry_type` on the backend but does not mention updating the `Config` and `HistoryEntry` interfaces in `App.tsx`, so TypeScript will fail to compile.
Fix: extend the `Config` and `HistoryEntry` interfaces and the default config object to include the new fields.

VERDICT: REVISE

### Claude's response

All 12 findings accepted and incorporated into PLAN.md:

- Added clipboard sentinel to distinguish real selection from stale clipboard text.
- Moved clipboard capture to shortcut RELEASE, after recording stops, to avoid key-state conflicts.
- Kept selection intact during recording; delete it only immediately before typing the result.
- Specified atomic ActiveMode transition under a single mutex lock.
- Specified emitting `transcription-result` after edit completion to refresh main window history/stats.
- Explicitly added `#[serde(default)]` to `config_version`.
- Explicitly added `#[serde(default)]` to `HistoryEntry.entry_type`, defaulting old entries to `Transcribe`.
- Made `set_config` accept `AppHandle` and re-register global shortcuts at runtime.
- Added validation that `shortcut` and `edit_shortcut` cannot be identical.
- Applied dictionary replacement to edit-mode instruction text before `llm::edit`.
- Used cleanup closure / scopeguard to always restore clipboard on success or failure.
- Updated TypeScript `Config` and `HistoryEntry` interfaces and default state.

Note: the `codex exec resume` command did not return output in this environment, so Round 2 was run as a fresh `codex exec -s read-only` session.

## Round 3 — Codex

I read the current plan and the two prior review rounds, then inspected the Rust and TS source to verify how the existing system actually works. The prior 23 findings are all reflected in the revised plan. Below are new concrete flaws that remain.

**New concrete flaws**

1. **Substring误伤 is not actually prevented.** The plan claims “子串不误伤”, but sorting by length and doing a single non-overlapping pass only prevents cascading; it still replaces “拉斯特” inside “拉斯特级” if no longer dictionary entry matches.  
   Fix: change the claim to “non-overlapping, longest-match only” and document that substring replacement can still occur, or add word/segment-boundary checks before replacing.

2. **English whole-word boundary switch has no Config field.** The plan says English can “通过配置开关启用整词边界（默认关闭）”, but no such field is added to `Config`.  
   Fix: add `dictionary_whole_word: bool` to `Config` with `#[serde(default)]`, or remove the whole-word claim from V3.

3. **`Cmd+C` on release may still fire with Shift held.** The `Released` event fires when the Period key is released, but Command and Shift may still be pressed, so the simulated copy becomes `Cmd+Shift+C` instead of `Cmd+C` in many apps.  
   Fix: wait until all modifier keys are up before simulating `Cmd+C`, or explicitly release Shift with enigo first.

4. **Empty instruction is not handled.** If the user releases the edit shortcut without speaking, the flow will still call `llm::edit`, delete the current selection, and type nothing.  
   Fix: abort the edit flow if `instruction_text.trim().is_empty()`.

5. **`HistoryStats` and `export_to_markdown` are not updated for `EntryType`.** The plan says stats and export should distinguish types, but the current `HistoryStats` struct and the export function have no place for it; edits will silently inflate transcription counts.  
   Fix: update `HistoryStats` to include `transcribe_words/today_words/sessions` and `edit_words/today_words/sessions`, and update `export_to_markdown` to label edit entries.

6. **`recording-state` event carries no mode, so the bubble cannot show edit-specific text.** The current `RecordingStateEvent` only has a `state` string.  
   Fix: add `mode: String` to `RecordingStateEvent` and emit state+mode combinations.

7. **Esc cancellation is not reconciled with `ActiveMode`.** The current `Esc` handler checks `state.recorder.lock().unwrap().is_some()`; with the new state it needs to check `active_mode != Idle` and cancel both Transcribe and Edit.  
   Fix: update the `Esc` handler to inspect `active_mode` and call a shared `cancel_active` for both modes.

8. **`ActiveMode` cannot replace the `recorder` field — both are needed.** The plan says “用 ActiveMode 枚举替代单一的 recorder 判断”, but the recorder handle must still be stored somewhere.  
   Fix: keep `recorder` and add `active_mode` inside a single `Mutex<SessionState>` so the idle check and transition are atomic.

9. **Dictionary validation is frontend-only.** A malformed `config.toml` can contain empty `from` or duplicate entries, which the backend will silently apply.  
   Fix: in `set_config` and `apply_dictionary`, skip entries with empty `from` and reject duplicates.

10. **`scopeguard` is not declared in `Cargo.toml`.** The plan mentions using `scopeguard` for clipboard cleanup, but the dependency is absent.  
    Fix: add `scopeguard = "1"` to `src-tauri/Cargo.toml`, or commit to a manual `defer` cleanup closure.

11. **Shortcut conflict with Esc is not rejected.** The plan only checks that `shortcut` and `edit_shortcut` differ, but a user could set `edit_shortcut` to `Escape` and collide with the hardcoded cancel shortcut.  
    Fix: also reject any shortcut whose key is `Escape` and modifiers are empty.

12. **Shortcut parser normalization is unspecified.** The existing default uses “Command+Period”, but users may write “Command+Shift+.” or “Shift+Command+Period”; string comparison alone would miss that they parse to the same shortcut.  
    Fix: compare parsed `Shortcut` instances after normalizing “.” to “Period” and canonicalizing modifier order.

13. **Case sensitivity for English dictionary entries is not specified.** Whether “rust” matches “Rust” or “RUST” changes behavior and is left undefined.  
    Fix: document and implement case-insensitive matching for ASCII entries.

14. **Dictionary replacement happens even in verbatim output mode.** The plan applies `apply_dictionary` before the `output_mode` check, so verbatim output will still be rewritten by dictionary entries.  
    Fix: explicitly decide whether dictionary applies in verbatim mode and document the choice; if not, move the check after the `output_mode` branch.

15. **`raw_text` format for edit entries is vague.** The plan says “raw_text 存指令和选中文本摘要” without a concrete format, which will break parsing later.  
    Fix: define the format, e.g. `raw_text = format!("instruction: {} | selected: {}", instruction, selected)` with a length cap.

16. **Main-window status text is not updated for edit mode.** The current `App.tsx` shows “识别完成并已输入” on `transcription-result`; after an edit it will incorrectly say the same thing.  
    Fix: include `entry_type` in the event payload or distinguish the status message so edits display “编辑完成并已输入”.

VERDICT: REVISE

### Claude's response

All 16 findings accepted and incorporated into PLAN.md:

- Replaced “子串不误伤” claim with “longest-match non-overlapping, cascades prevented; substring matches inside longer words can still occur” and documented the limitation.
- Removed the English whole-word boundary switch claim from V3 to keep scope controlled.
- Added explicit release of edit shortcut modifier keys before simulating `Cmd+C`.
- Added empty-instruction abort before deleting selection or calling LLM.
- Split `HistoryStats` into `transcribe_*` and `edit_*` counters; updated `export_to_markdown` to label entry types.
- Added `mode` field to `RecordingStateEvent` and specified bubble state+mode display.
- Reconciled `Esc` cancellation with `ActiveMode` and added shared `cancel_active`.
- Replaced “replace recorder field” wording with `Mutex<SessionState>` holding both `active_mode` and `recorder`.
- Added backend dictionary validation (skip empty `from`, reject duplicates) with shared validation helper.
- Added `scopeguard = "1"` dependency to `Cargo.toml`.
- Added rejection of bare `Escape` shortcut to avoid collision with cancel shortcut.
- Specified shortcut parser normalization for aliases and modifier order.
- Documented case-insensitive matching for ASCII dictionary entries.
- Explicitly decided dictionary applies in verbatim mode and documented the rationale.
- Defined fixed `raw_text` format for edit entries with length cap.
- Added `entry_type` to `TranscriptionEvent` and specified distinct status messages for edit vs transcribe.

Note: `codex exec resume` is not usable in this environment; Round 3 was run as a fresh `codex exec -s read-only` session.

## Round 4 — Codex

Prior findings from Rounds 1–3 are reflected in the current plan. New concrete flaws remain:

1. **Default edit shortcut conflicts with macOS.** `Command+Shift+Period` (`⌘+Shift+.`) is the system shortcut to show/hide hidden files in Finder and open/save dialogs, so registration will likely fail or be intercepted.
   Fix: change the default to `Command+Option+Period` or another non-conflicting combo.

2. **The plan does not explicitly rewrite the hardcoded `handle_shortcut` dispatcher.** The existing `lib.rs` handler only recognizes `Code::Period + Modifiers::SUPER`; even after parsing config shortcuts, the new edit shortcut will be ignored unless the dispatch logic is rewritten.
   Fix: rewrite `handle_shortcut` to compare the incoming `Shortcut` against the parsed `shortcut` and `edit_shortcut` from config.

3. **The main `shortcut` can still be set to bare `Escape`.** The plan only rejects bare `Escape` for `edit_shortcut`, but setting the transcription shortcut to `Escape` would collide with the hardcoded cancel shortcut.
   Fix: reject bare `Escape` for both `shortcut` and `edit_shortcut`.

4. **Non-text clipboard content is not restored.** The plan saves the original clipboard only if it is text and restores only if it was text, so an original image/file on the clipboard will be replaced by the sentinel or selected text.
   Fix: save and restore the original clipboard content unconditionally, using the platform clipboard API for non-text data.

5. **`src/dictionary.rs` is created but never declared as a module.** The plan says to create the file but does not mention adding `mod dictionary;` to `src/lib.rs`.
   Fix: add `mod dictionary;` in `src-tauri/src/lib.rs`.

6. **Key new types/functions are referenced but not defined.** The plan mentions `Mutex<SessionState>`, `ActiveMode`, and `cancel_active` but never gives their definitions or signatures.
   Fix: add concrete definitions for `SessionState`, `ActiveMode`, and `cancel_active` in the plan.

7. **Shortcut re-registration is unsafe.** The plan says to unregister the old shortcuts before registering the new ones; if registration fails, the app is left with no global shortcuts.
   Fix: register the new shortcuts first, then unregister the old ones, and roll back on failure.

8. **`config_version` default value is unspecified.** The plan adds `config_version: u32` but does not say what value to write in `Config::default()`.
   Fix: set `config_version: 1` in `Config::default()` and document it.

9. **Dictionary entries are not trimmed before validation.** A user could create both `"Rust"` and `" Rust "` as distinct entries, or a whitespace-only `from` could slip past the empty check.
   Fix: trim `from` and `to` before validation and before storing.

10. **`llm::edit` has no specified system prompt.** The plan says “use a system prompt” but does not provide the prompt text or the free-generation case for `selected_text: None`.
    Fix: include the exact `llm::edit` system prompt(s) and behavior for `selected_text: None` in the plan.

11. **`stop_recording` is not refactored for edit mode.** The plan describes an edit flow on shortcut release but does not say whether to parameterize `stop_recording` by mode or add a dedicated `stop_edit_recording`.
    Fix: parameterize the stop handler by `ActiveMode` or add a separate `stop_edit_recording` function.

12. **`emit_result` and `emit_state` are not updated.** The plan adds `entry_type` and `mode` to event payloads but does not explicitly say to update these helper functions in `lib.rs`.
    Fix: update `emit_result` to include `entry_type` and `emit_state` to include `mode`.

13. **Edit mode’s relationship to `recording_mode` is undefined.** The plan does not state whether edit mode obeys the existing `continuous` / `push_to_talk` setting; it should always be press-and-release.
    Fix: explicitly state that edit mode always uses press-and-release, independent of `recording_mode`.

14. **Failed edits are not accounted for.** The plan does not say whether a failed edit (ASR/LLM/type error) is saved to history or what status/error is emitted.
    Fix: specify that failed edits emit an error event and are not saved to history.

15. **Shortcut parser case-sensitivity is undefined.** The plan normalizes aliases like `Command`/`Cmd` but does not say whether `"command+period"` and `"Command+Period"` parse to the same shortcut.
    Fix: make `parse_shortcut` case-insensitive.

VERDICT: REVISE

### Claude's response

All 15 findings accepted and incorporated into PLAN.md:

- Changed default edit shortcut from `Command+Shift+Period` to `Command+Option+Period` to avoid macOS Finder conflict.
- Explicitly stated `handle_shortcut` will be rewritten to compare incoming shortcut against parsed `shortcut` and `edit_shortcut`.
- Extended bare `Escape` rejection to both `shortcut` and `edit_shortcut`.
- Expanded clipboard save/restore to handle text and image data via `arboard`; documented other non-text formats as known limitation.
- Added explicit instruction to declare `mod dictionary;` in `src/lib.rs`.
- Added concrete definitions for `ActiveMode`, `SessionState`, and `cancel_active` in the plan.
- Changed shortcut re-registration order: register new first, then unregister old, with rollback on failure.
- Specified `config_version: 1` as default in `Config::default()`.
- Added trimming of dictionary `from`/`to` before validation and storage.
- Included exact `llm::edit` system prompt and message structure, including `selected_text: None` behavior.
- Specified parameterizing `stop_recording` by `ActiveMode` or splitting into separate pipeline functions.
- Explicitly updated `emit_state` and `emit_result` helper signatures to include `mode` and `entry_type`.
- Explicitly stated edit mode always uses press-and-release, independent of `recording_mode`.
- Specified failed edits emit `error` event and are not saved to history.
- Made `parse_shortcut` case-insensitive.

Note: `codex exec resume` is not usable in this environment; Round 4 was run as a fresh `codex exec -s read-only` session.

## Round 5 — Codex

Prior findings from Rounds 1–4 are reflected, but the plan still contains material gaps that would lead to implementation bugs or undefined behavior.

1. **Modifier keys are not released before typing the edit result.** The plan releases Command/Option before simulating `Cmd+C`, but the user may still be physically holding them while the LLM processes. When `type_text` finally runs, held modifiers can turn the typed result into shortcuts or garbage input.
   Fix: release all relevant modifiers via `enigo` immediately before calling `type_text`.

2. **`type_text` clipboard fallback overwrites the restored clipboard.** The plan restores the original clipboard right after selection capture, but if the edit result is long, `type_text` falls back to `Cmd+V` and leaves the edited text on the clipboard instead of the original content.
   Fix: restore the original clipboard again after `type_text` returns, or document that the clipboard will contain the result after long edits.

3. **Shortcut re-registration does not guard against an active session.** If the user changes shortcuts while transcribing or editing, `set_config` will re-register global shortcuts while a recording is in progress, potentially dropping the active shortcut or leaving the session stuck.
   Fix: reject `set_config` when `active_mode != Idle`, or hold the session lock across the entire config change and shortcut re-registration.

4. **Parsed shortcuts have no specified home.** The plan says `handle_shortcut` will compare the incoming `Shortcut` against parsed config shortcuts, but never says where those parsed values live.
   Fix: add `parsed_shortcut: Mutex<Option<Shortcut>>` and `parsed_edit_shortcut: Mutex<Option<Shortcut>>` to `AppState` and update them whenever config is loaded or saved.

5. **`RecordingStateEvent.mode` conflates state and mode.** The plan defines `mode` as `"transcribe" / "edit" / "idle"`, but `"idle"` is a state, not a mode. When `state == "idle"`, sending a mode is confusing and complicates the bubble state machine.
   Fix: make `mode` optional (`Option<String>`) and emit it only when `state != "idle"`.

6. **`duration_ms` for edit history entries is undefined.** The current `HistoryEntry` schema requires `duration_ms`. For edits, the plan does not say what value to store, leaving the schema incomplete.
   Fix: store the audio recording duration as `duration_ms` for edit entries, or add `#[serde(default)]` to `duration_ms` and set it to 0 for edits.

7. **`apply_dictionary` calling strict validation is ambiguous.** The plan says `set_config` and `apply_dictionary` both call `validate_dictionary`, but does not define what `apply_dictionary` does on validation failure. If it returns an error, a malformed dictionary can break every transcription.
   Fix: validate dictionary only at config save/load; make `apply_dictionary` assume valid entries and ignore malformed ones silently.

8. **Dictionary replacement algorithm is ambiguous.** "Sort by length and do a single non-overlapping pass" can be implemented as sequential entry replacement rather than longest-match at each position.
   Fix: specify the algorithm explicitly: at each position, choose the longest matching entry, replace it, and advance by the matched length.

VERDICT: REVISE

### Claude's response

All 8 findings accepted and incorporated into PLAN.md:

- Added explicit modifier-key release immediately before `type_text` in edit flow.
- Specified restoring original clipboard again after `type_text` returns, to counter clipboard-fallback side effect.
- Added active-session guard: `set_config` rejects when `active_mode != Idle`.
- Added `parsed_shortcut` and `parsed_edit_shortcut` to `AppState` and specified updating them on load/save.
- Changed `RecordingStateEvent.mode` to `Option<String>`; `None` when `state == "idle"`.
- Specified `duration_ms` for edit entries reuses the audio recording duration.
- Clarified dictionary validation only at save/load; `apply_dictionary` silently skips malformed entries.
- Rewrote dictionary algorithm description to explicitly state longest-match-at-each-position, non-overlapping, single-pass semantics.

Note: `codex exec resume` is not usable in this environment; Round 5 was run as a fresh `codex exec -s read-only` session. MAX_ROUNDS reached.
