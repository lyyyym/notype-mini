mod audio;
mod config;
mod dashscope;
mod dictionary;
mod history;
mod llm;

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

// 事件载荷结构
#[derive(Clone, serde::Serialize)]
struct RecordingStateEvent {
    state: String,
    mode: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct TranscriptionEvent {
    text: String,
    entry_type: String,
}

#[derive(Clone, serde::Serialize)]
struct ErrorEvent {
    code: String,
    message: String,
}

// 活跃模式
#[derive(Debug, Clone, Copy, PartialEq)]
enum ActiveMode {
    Idle,
    Transcribe,
    Edit,
}

// 录音模式
#[derive(Debug, Clone, Copy, PartialEq)]
enum RecordingMode {
    PushToTalk,
    Continuous,
}

impl From<&str> for RecordingMode {
    fn from(s: &str) -> Self {
        match s {
            "continuous" => Self::Continuous,
            _ => Self::PushToTalk,
        }
    }
}

// 会话状态
struct SessionState {
    active_mode: ActiveMode,
    recorder: Option<audio::RecorderHandle>,
}

// 应用状态
struct AppState {
    config: Mutex<config::Config>,
    session: Mutex<SessionState>,
    parsed_shortcut: Mutex<Option<Shortcut>>,
    parsed_edit_shortcut: Mutex<Option<Shortcut>>,
}

// 命令：获取配置
#[tauri::command]
fn get_config(state: State<AppState>) -> config::Config {
    state.config.lock().unwrap().clone()
}

// 命令：保存配置
#[tauri::command]
fn set_config(
    app: AppHandle,
    new_config: config::Config,
    state: State<AppState>,
) -> Result<(), String> {
    // 检查当前是否空闲
    {
        let session = state.session.lock().unwrap();
        if session.active_mode != ActiveMode::Idle {
            return Err("请先结束当前录音或编辑再修改设置".to_string());
        }
    }

    // 验证词典
    if let Err(e) = dictionary::validate_dictionary(&new_config.dictionary) {
        return Err(e);
    }

    // 解析并校验快捷键
    let new_parsed = parse_shortcut(&new_config.shortcut)?;
    let new_parsed_edit = parse_shortcut(&new_config.edit_shortcut)?;

    // 校验不是裸 Escape
    if is_bare_escape(&new_parsed) {
        return Err("录音快捷键不能是 Esc".to_string());
    }
    if is_bare_escape(&new_parsed_edit) {
        return Err("编辑快捷键不能是 Esc".to_string());
    }

    // 校验两者不相同
    if new_parsed == new_parsed_edit {
        return Err("录音快捷键和编辑快捷键不能相同".to_string());
    }

    // 如果快捷键没有变化，直接保存配置即可，避免重复注册导致 macOS 热键注册失败
    let old_parsed = *state.parsed_shortcut.lock().unwrap();
    let old_parsed_edit = *state.parsed_edit_shortcut.lock().unwrap();

    if Some(new_parsed) == old_parsed && Some(new_parsed_edit) == old_parsed_edit {
        new_config.save().map_err(|e| e.to_string())?;
        *state.config.lock().unwrap() = new_config;
        return Ok(());
    }

    // 只注册发生变化的快捷键；先注册新的，成功后再注销旧的
    let mut registered_shortcut = false;
    if Some(new_parsed) != old_parsed {
        app.global_shortcut()
            .register(new_parsed)
            .map_err(|e| format!("注册录音快捷键失败: {}", e))?;
        registered_shortcut = true;
    }

    if Some(new_parsed_edit) != old_parsed_edit {
        if let Err(e) = app.global_shortcut().register(new_parsed_edit) {
            // 回滚已注册的录音快捷键
            if registered_shortcut {
                let _ = app.global_shortcut().unregister(new_parsed);
            }
            return Err(format!("注册编辑快捷键失败: {}", e));
        }
    }

    *state.parsed_shortcut.lock().unwrap() = Some(new_parsed);
    *state.parsed_edit_shortcut.lock().unwrap() = Some(new_parsed_edit);

    // 注销旧快捷键
    if let Some(old) = old_parsed {
        if old != new_parsed {
            let _ = app.global_shortcut().unregister(old);
        }
    }
    if let Some(old) = old_parsed_edit {
        if old != new_parsed_edit {
            let _ = app.global_shortcut().unregister(old);
        }
    }

    // 所有变更成功后持久化配置
    new_config.save().map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = new_config;
    Ok(())
}

fn is_bare_escape(shortcut: &Shortcut) -> bool {
    shortcut.key == Code::Escape && shortcut.mods == Modifiers::empty()
}

// 命令：测试 DashScope 配置
#[tauri::command]
async fn test_dashscope_config(config: dashscope::DashScopeConfig) -> Result<String, String> {
    dashscope::test_connection(&config)
        .await
        .map_err(|e| e.to_string())
}

// 命令：测试 LLM 配置
#[tauri::command]
async fn test_llm_config(config: llm::LlmConfig) -> Result<String, String> {
    llm::test_connection(&config)
        .await
        .map_err(|e| e.to_string())
}

// 命令：获取历史记录
#[tauri::command]
fn get_history(limit: Option<usize>) -> Vec<history::HistoryEntry> {
    history::get_entries(limit)
}

// 命令：删除历史记录
#[tauri::command]
fn delete_history_item(id: String) -> Result<(), String> {
    history::delete_entry(&id).map_err(|e| e.to_string())
}

// 命令：清空历史记录
#[tauri::command]
fn clear_history() -> Result<(), String> {
    history::clear_history().map_err(|e| e.to_string())
}

// 命令：导出历史记录为 Markdown
#[tauri::command]
fn export_history() -> Result<String, String> {
    history::export_to_markdown().map_err(|e| e.to_string())
}

// 命令：获取统计
#[tauri::command]
fn get_stats() -> history::HistoryStats {
    history::get_stats()
}

// 工具函数：发送事件
fn emit_state(app: &AppHandle, state: &str, mode: Option<&str>) {
    let _ = app.emit(
        "recording-state",
        RecordingStateEvent {
            state: state.to_string(),
            mode: mode.map(|s| s.to_string()),
        },
    );
}

fn emit_result(app: &AppHandle, text: String, entry_type: history::EntryType) {
    let _ = app.emit(
        "transcription-result",
        TranscriptionEvent {
            text,
            entry_type: match entry_type {
                history::EntryType::Transcribe => "transcribe".to_string(),
                history::EntryType::Edit => "edit".to_string(),
            },
        },
    );
}

fn emit_error(app: &AppHandle, code: &str, message: String) {
    let _ = app.emit(
        "error",
        ErrorEvent {
            code: code.to_string(),
            message,
        },
    );
}

// 显示/隐藏气泡窗口
fn show_bubble(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("bubble") {
        let _ = window.center();
        let _ = window.show();
        // 注意：不要调用 set_focus，否则气泡会抢走当前应用焦点
    }
}

fn hide_bubble(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("bubble") {
        let _ = window.hide();
    }
}

// 设置系统托盘
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let hide_i = MenuItem::with_id(app, "hide", "隐藏主窗口", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_i,
            &hide_i,
            &PredefinedMenuItem::separator(app)?,
            &quit_i,
        ],
    )?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                "hide" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

// 解析快捷键字符串
fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return Err("快捷键格式错误".to_string());
    }

    let key_part = parts.last().unwrap().to_lowercase();
    let key = normalize_key(&key_part).ok_or_else(|| format!("不支持的按键: {}", key_part))?;

    let mut mods = Modifiers::empty();
    for part in &parts[..parts.len() - 1] {
        let part_lower = part.to_lowercase();
        match part_lower.as_str() {
            "command" | "cmd" | "super" | "win" => mods |= Modifiers::SUPER,
            "control" | "ctrl" => mods |= Modifiers::CONTROL,
            "option" | "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            _ => return Err(format!("不支持的修饰键: {}", part)),
        }
    }

    // 修饰符顺序标准化：Tauri 内部会比较，这里构造出来即可
    let shortcut = if mods.is_empty() {
        Shortcut::new(None, key)
    } else {
        Shortcut::new(Some(mods), key)
    };

    Ok(shortcut)
}

fn normalize_key(s: &str) -> Option<Code> {
    match s {
        "period" | "." => Some(Code::Period),
        "comma" | "," => Some(Code::Comma),
        "semicolon" | ";" => Some(Code::Semicolon),
        "quote" | "'" | "\"" => Some(Code::Quote),
        "slash" | "/" => Some(Code::Slash),
        "backslash" | "\\" => Some(Code::Backslash),
        "bracketleft" | "[" => Some(Code::BracketLeft),
        "bracketright" | "]" => Some(Code::BracketRight),
        "minus" | "-" => Some(Code::Minus),
        "equal" | "=" => Some(Code::Equal),
        "escape" | "esc" => Some(Code::Escape),
        "space" => Some(Code::Space),
        "return" | "enter" => Some(Code::Enter),
        "tab" => Some(Code::Tab),
        "backspace" => Some(Code::Backspace),
        "delete" | "del" => Some(Code::Delete),
        "home" => Some(Code::Home),
        "end" => Some(Code::End),
        "pageup" => Some(Code::PageUp),
        "pagedown" => Some(Code::PageDown),
        "left" => Some(Code::ArrowLeft),
        "right" => Some(Code::ArrowRight),
        "up" => Some(Code::ArrowUp),
        "down" => Some(Code::ArrowDown),
        "a" => Some(Code::KeyA),
        "b" => Some(Code::KeyB),
        "c" => Some(Code::KeyC),
        "d" => Some(Code::KeyD),
        "e" => Some(Code::KeyE),
        "f" => Some(Code::KeyF),
        "g" => Some(Code::KeyG),
        "h" => Some(Code::KeyH),
        "i" => Some(Code::KeyI),
        "j" => Some(Code::KeyJ),
        "k" => Some(Code::KeyK),
        "l" => Some(Code::KeyL),
        "m" => Some(Code::KeyM),
        "n" => Some(Code::KeyN),
        "o" => Some(Code::KeyO),
        "p" => Some(Code::KeyP),
        "q" => Some(Code::KeyQ),
        "r" => Some(Code::KeyR),
        "s" => Some(Code::KeyS),
        "t" => Some(Code::KeyT),
        "u" => Some(Code::KeyU),
        "v" => Some(Code::KeyV),
        "w" => Some(Code::KeyW),
        "x" => Some(Code::KeyX),
        "y" => Some(Code::KeyY),
        "z" => Some(Code::KeyZ),
        "0" => Some(Code::Digit0),
        "1" => Some(Code::Digit1),
        "2" => Some(Code::Digit2),
        "3" => Some(Code::Digit3),
        "4" => Some(Code::Digit4),
        "5" => Some(Code::Digit5),
        "6" => Some(Code::Digit6),
        "7" => Some(Code::Digit7),
        "8" => Some(Code::Digit8),
        "9" => Some(Code::Digit9),
        _ => None,
    }
}

// 注册两个全局快捷键
fn register_shortcuts(
    app: &AppHandle,
    shortcut: &Shortcut,
    edit_shortcut: &Shortcut,
) -> Result<(), String> {
    app.global_shortcut()
        .register(*shortcut)
        .map_err(|e| format!("注册录音快捷键失败: {}", e))?;
    app.global_shortcut()
        .register(*edit_shortcut)
        .map_err(|e| {
            // 回滚第一个
            let _ = app.global_shortcut().unregister(*shortcut);
            format!("注册编辑快捷键失败: {}", e)
        })?;
    Ok(())
}

// 取消当前会话
fn cancel_active(app: &AppHandle, state: State<AppState>) {
    let handle = {
        let mut session = state.session.lock().unwrap();
        session.active_mode = ActiveMode::Idle;
        session.recorder.take()
    };

    hide_bubble(app);
    emit_state(app, "idle", None);

    if let Some(handle) = handle {
        tauri::async_runtime::spawn(async move {
            let _ = handle.stop();
        });
    }
}

// 开始录音
fn start_recording(app: &AppHandle, state: State<AppState>, mode: ActiveMode) {
    let app_handle = app.clone();

    match audio::start_recording(app_handle) {
        Ok(handle) => {
            {
                let mut session = state.session.lock().unwrap();
                session.active_mode = mode;
                session.recorder = Some(handle);
            }
            show_bubble(app);
            let mode_str = match mode {
                ActiveMode::Transcribe => Some("transcribe"),
                ActiveMode::Edit => Some("edit"),
                ActiveMode::Idle => None,
            };
            emit_state(app, "recording", mode_str);
        }
        Err(e) => {
            eprintln!("开始录音失败: {}", e);
            emit_error(app, "recording_start_failed", e.to_string());
            emit_state(app, "idle", None);
        }
    }
}

// 设置剪贴板内容
fn set_clipboard_text(text: &str) -> Result<(), anyhow::Error> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| anyhow::anyhow!("剪贴板初始化失败: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| anyhow::anyhow!("复制到剪贴板失败: {}", e))?;
    Ok(())
}

// 释放所有修饰键
fn release_modifiers(enigo: &mut enigo::Enigo) -> Result<(), anyhow::Error> {
    use enigo::{Direction::Release, Key, Keyboard};

    let _ = enigo.key(Key::Meta, Release);
    let _ = enigo.key(Key::Shift, Release);
    let _ = enigo.key(Key::Alt, Release);
    let _ = enigo.key(Key::Control, Release);
    Ok(())
}

// 文字输入到光标位置
fn type_text(
    text: &str,
    auto_enter: bool,
    config: &config::Config,
    enigo: &mut Option<enigo::Enigo>,
) -> Result<(), anyhow::Error> {
    use enigo::{
        Direction::Click, Direction::Press, Direction::Release, Enigo, Key, Keyboard, Settings,
    };

    let enigo = match enigo {
        Some(e) => e,
        None => {
            *enigo = Some(Enigo::new(&Settings::default())?);
            enigo.as_mut().unwrap()
        }
    };

    let use_fallback = config.use_clipboard_fallback;
    let threshold = config.clipboard_fallback_threshold;

    let mut used_clipboard = false;

    if use_fallback && text.chars().count() > threshold {
        set_clipboard_text(text)?;
        used_clipboard = true;
    } else {
        if let Err(e) = enigo.text(text) {
            if use_fallback {
                eprintln!("键盘输入失败，回退到剪贴板粘贴: {:?}", e);
                set_clipboard_text(text)?;
                used_clipboard = true;
            } else {
                return Err(anyhow::anyhow!("键盘输入失败: {:?}", e));
            }
        }
    }

    if used_clipboard {
        // macOS: Cmd+V
        enigo.key(Key::Meta, Press)?;
        enigo.key(Key::Unicode('v'), Click)?;
        enigo.key(Key::Meta, Release)?;
    }

    if auto_enter {
        enigo
            .key(Key::Return, Click)
            .map_err(|e| anyhow::anyhow!("回车失败: {:?}", e))?;
    }

    Ok(())
}

// 停止录音并处理识别
fn stop_recording(app: &AppHandle, state: State<AppState>) {
    // 取出 recorder 和当前模式
    let (handle, mode) = {
        let mut session = state.session.lock().unwrap();
        let mode = session.active_mode;
        session.active_mode = ActiveMode::Idle;
        (session.recorder.take(), mode)
    };

    if let Some(handle) = handle {
        hide_bubble(app);
        emit_state(app, "processing", None);

        let app_clone = app.clone();
        let config = state.config.lock().unwrap().clone();

        // 在异步任务中处理收尾、识别、润色、输入
        tauri::async_runtime::spawn(async move {
            match handle.stop() {
                Ok((samples, sample_rate)) => {
                    if samples.is_empty() {
                        emit_error(
                            &app_clone,
                            "no_audio",
                            "未录制到音频".to_string(),
                        );
                        emit_state(&app_clone, "idle", None);
                        return;
                    }

                    let duration_ms =
                        (samples.len() as f64 / sample_rate as f64 * 1000.0) as u64;

                    match audio::samples_to_wav(&samples, sample_rate) {
                        Ok(wav_bytes) => {
                            match mode {
                                ActiveMode::Transcribe => {
                                    transcribe_pipeline(
                                        app_clone,
                                        config,
                                        wav_bytes,
                                        duration_ms,
                                    )
                                    .await;
                                }
                                ActiveMode::Edit => {
                                    edit_pipeline(
                                        app_clone,
                                        config,
                                        wav_bytes,
                                        duration_ms,
                                    )
                                    .await;
                                }
                                ActiveMode::Idle => {
                                    emit_state(&app_clone, "idle", None);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("WAV 编码失败: {}", e);
                            emit_error(
                                &app_clone,
                                "wav_encode_failed",
                                e.to_string(),
                            );
                            emit_state(&app_clone, "idle", None);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("停止录音失败: {}", e);
                    emit_error(
                        &app_clone,
                        "recording_stop_failed",
                        e,
                    );
                    emit_state(&app_clone, "idle", None);
                }
            }
        });
    }
}

// 转写流程
async fn transcribe_pipeline(
    app: AppHandle,
    config: config::Config,
    wav_bytes: Vec<u8>,
    duration_ms: u64,
) {
    match dashscope::transcribe(wav_bytes, &config.dashscope).await {
        Ok(raw_text) => {
            // 应用个人词典
            let raw_text = dictionary::apply_dictionary(&raw_text, &config.dictionary);

            // 根据输出模式决定是否润色
            let final_text = if config.output_mode == "polish" {
                match llm::polish(&raw_text, &config.llm).await {
                    Ok(polished) => polished,
                    Err(e) => {
                        eprintln!("润色失败，使用原始识别结果: {}", e);
                        raw_text.clone()
                    }
                }
            } else {
                raw_text.clone()
            };

            // 输入到光标位置
            let mut enigo = None;
            if let Err(e) = type_text(&final_text, config.auto_enter, &config, &mut enigo) {
                eprintln!("输入失败: {}", e);
                emit_error(
                    &app,
                    "input_failed",
                    format!("无法输入文字: {}", e),
                );
            }

            // 保存到历史记录
            let entry = history::HistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: chrono::Local::now(),
                raw_text: raw_text.clone(),
                polished_text: final_text.clone(),
                word_count: final_text.chars().count(),
                duration_ms,
                entry_type: history::EntryType::Transcribe,
            };
            if let Err(e) = history::add_entry(entry) {
                eprintln!("保存历史记录失败: {}", e);
            }

            emit_result(&app, final_text, history::EntryType::Transcribe);
            emit_state(&app, "idle", None);
        }
        Err(e) => {
            eprintln!("识别失败: {}", e);
            emit_error(
                &app,
                "transcription_failed",
                e.to_string(),
            );
            emit_state(&app, "idle", None);
        }
    }
}

// 编辑流程
async fn edit_pipeline(
    app: AppHandle,
    config: config::Config,
    wav_bytes: Vec<u8>,
    duration_ms: u64,
) {
    // 1. 识别指令
    let instruction = match dashscope::transcribe(wav_bytes, &config.dashscope).await {
        Ok(text) => dictionary::apply_dictionary(&text, &config.dictionary),
        Err(e) => {
            eprintln!("指令识别失败: {}", e);
            emit_error(&app,
                "instruction_recognition_failed",
                e.to_string(),
            );
            emit_state(&app, "idle", None);
            return;
        }
    };

    if instruction.trim().is_empty() {
        emit_state(&app, "idle", None);
        return;
    }

    // 2. 获取选中文本
    let (original_clipboard, selected_text) =
        match capture_selected_text(&config.edit_shortcut,
        ) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("获取选中文本失败: {}", e);
                emit_error(
                    &app,
                    "clipboard_capture_failed",
                    e.to_string(),
                );
                emit_state(&app, "idle", None);
                return;
            }
        };

    // scopeguard 保证剪贴板恢复
    let app_for_cleanup = app.clone();
    let _cleanup = scopeguard::guard(original_clipboard.clone(), |original| {
        restore_clipboard(original, &app_for_cleanup);
    });

    // 3. 调用 LLM 编辑
    let result_text = match llm::edit(
        &instruction,
        selected_text.as_deref(),
        &config.llm,
    )
    .await
    {
        Ok(text) => text,
        Err(e) => {
            eprintln!("编辑失败: {}", e);
            emit_error(&app, "edit_failed", e.to_string());
            emit_state(&app, "idle", None);
            return;
        }
    };

    // 4. 输入结果
    let mut enigo = match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(e) => Some(e),
        Err(e) => {
            eprintln!("创建 Enigo 失败: {:?}", e);
            emit_error(
                &app,
                "input_failed",
                format!("无法创建键盘输入: {:?}", e),
            );
            emit_state(&app, "idle", None);
            return;
        }
    };

    if let Some(ref mut e) = enigo {
        if let Err(err) = release_modifiers(e) {
            eprintln!("释放修饰键失败: {}", err);
        }
    }

    // 删除选区
    if selected_text.is_some() {
        if let Some(ref mut e) = enigo {
            use enigo::{Direction::Click, Key, Keyboard};
            if let Err(err) = e.key(Key::Delete, Click) {
                eprintln!("删除选区失败: {:?}", err);
            }
        }
    }

    if let Err(e) = type_text(
        &result_text, false, &config, &mut enigo
    ) {
        eprintln!("编辑结果输入失败: {}", e);
        emit_error(
            &app,
            "input_failed",
            format!("无法输入编辑结果: {}", e),
        );
        emit_state(&app, "idle", None);
        return;
    }

    // 再次恢复剪贴板（type_text 的 clipboard fallback 可能覆盖了它）
    drop(_cleanup);
    restore_clipboard(original_clipboard, &app);

    // 5. 保存到历史记录
    let raw_text = format!(
        "instruction: {} | selected: {}",
        &instruction,
        selected_text.as_deref().unwrap_or("<无>")
    );
    let capped_raw_text = if raw_text.chars().count() > 250 {
        let mut s: String = raw_text.chars().take(250).collect();
        s.push_str("...");
        s
    } else {
        raw_text
    };

    let entry = history::HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Local::now(),
        raw_text: capped_raw_text,
        polished_text: result_text.clone(),
        word_count: result_text.chars().count(),
        duration_ms,
        entry_type: history::EntryType::Edit,
    };
    if let Err(e) = history::add_entry(entry) {
        eprintln!("保存编辑历史记录失败: {}", e);
    }

    emit_result(&app, result_text, history::EntryType::Edit);
    emit_state(&app, "idle", None);
}

// 原始剪贴板内容
#[derive(Clone)]
enum OriginalClipboard {
    Text(String),
    Image(arboard::ImageData<'static>),
    None,
}

// 捕获选中文本
fn capture_selected_text(
    edit_shortcut_str: &str,
) -> Result<(OriginalClipboard, Option<String>), anyhow::Error> {
    use enigo::{Direction::Release, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())?;

    // 根据编辑快捷键释放可能仍按下的修饰键
    let parsed = parse_shortcut(edit_shortcut_str)
        .map_err(|e| anyhow::anyhow!("解析编辑快捷键失败: {}", e))?;
    if parsed.mods.contains(Modifiers::SUPER) {
        let _ = enigo.key(Key::Meta, Release);
    }
    if parsed.mods.contains(Modifiers::SHIFT) {
        let _ = enigo.key(Key::Shift, Release);
    }
    if parsed.mods.contains(Modifiers::ALT) {
        let _ = enigo.key(Key::Alt, Release);
    }
    if parsed.mods.contains(Modifiers::CONTROL) {
        let _ = enigo.key(Key::Control, Release);
    }

    // 保存原始剪贴板
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| anyhow::anyhow!("剪贴板初始化失败: {}", e))?;

    let original = match clipboard.get_text() {
        Ok(text) => OriginalClipboard::Text(text),
        Err(_) => match clipboard.get_image() {
            Ok(img) => OriginalClipboard::Image(img),
            Err(_) => OriginalClipboard::None,
        },
    };

    // 设置 sentinel
    let sentinel = format!("__NOTYPE_CLIPBOARD_SENTINEL_{}__", uuid::Uuid::new_v4());
    clipboard
        .set_text(&sentinel)
        .map_err(|e| anyhow::anyhow!("设置 sentinel 失败: {}", e))?;

    // 模拟 Cmd+C
    enigo.key(Key::Meta, enigo::Direction::Press)?;
    enigo.key(Key::Unicode('c'), enigo::Direction::Click)?;
    enigo.key(Key::Meta, enigo::Direction::Release)?;

    // 轮询剪贴板
    let mut selected_text: Option<String> = None;
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(500) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        if let Ok(text) = clipboard.get_text() {
            if text != sentinel {
                selected_text = Some(text);
                break;
            }
        }
    }

    // 立即恢复原始剪贴板
    restore_clipboard_internal(&original, &mut clipboard)?;

    Ok((original, selected_text))
}

fn restore_clipboard(original: OriginalClipboard, _app: &AppHandle) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("恢复剪贴板失败: {}", e);
            return;
        }
    };
    if let Err(e) = restore_clipboard_internal(&original, &mut clipboard) {
        eprintln!("恢复剪贴板失败: {}", e);
    }
}

fn restore_clipboard_internal(
    original: &OriginalClipboard,
    clipboard: &mut arboard::Clipboard,
) -> Result<(), anyhow::Error> {
    match original {
        OriginalClipboard::Text(text) => {
            clipboard
                .set_text(text)
                .map_err(|e| anyhow::anyhow!("恢复文本剪贴板失败: {}", e))?;
        }
        OriginalClipboard::Image(img) => {
            clipboard
                .set_image(img.clone())
                .map_err(|e| anyhow::anyhow!("恢复图片剪贴板失败: {}", e))?;
        }
        OriginalClipboard::None => {
            // 没有可恢复的内容
        }
    }
    Ok(())
}

// 全局快捷键处理函数
fn handle_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    // Esc 取消当前会话
    if shortcut.key == Code::Escape && event.state() == ShortcutState::Pressed {
        let state: State<AppState> = app.state();
        if state.session.lock().unwrap().active_mode != ActiveMode::Idle {
            cancel_active(app, state);
        }
        return;
    }

    let state: State<AppState> = app.state();
    let parsed_shortcut = *state.parsed_shortcut.lock().unwrap();
    let parsed_edit_shortcut = *state.parsed_edit_shortcut.lock().unwrap();

    // 匹配录音快捷键
    if Some(*shortcut) == parsed_shortcut {
        let mode = RecordingMode::from(
            state.config.lock().unwrap().recording_mode.as_str(),
        );

        match mode {
            RecordingMode::Continuous => {
                if event.state() == ShortcutState::Pressed {
                    let session = state.session.lock().unwrap();
                    if session.active_mode == ActiveMode::Idle {
                        drop(session);
                        start_recording(app, state, ActiveMode::Transcribe);
                    } else if session.active_mode == ActiveMode::Transcribe {
                        drop(session);
                        stop_recording(app, state);
                    }
                }
            }
            RecordingMode::PushToTalk => {
                match event.state() {
                    ShortcutState::Pressed => {
                        let mut session = state.session.lock().unwrap();
                        if session.active_mode == ActiveMode::Idle {
                            session.active_mode = ActiveMode::Transcribe;
                            drop(session);
                            start_recording(app, state, ActiveMode::Transcribe);
                        }
                    }
                    ShortcutState::Released => {
                        let session = state.session.lock().unwrap();
                        if session.active_mode == ActiveMode::Transcribe {
                            drop(session);
                            stop_recording(app, state);
                        }
                    }
                }
            }
        }
        return;
    }

    // 匹配编辑快捷键
    if Some(*shortcut) == parsed_edit_shortcut {
        match event.state() {
            ShortcutState::Pressed => {
                let mut session = state.session.lock().unwrap();
                if session.active_mode == ActiveMode::Idle {
                    session.active_mode = ActiveMode::Edit;
                    drop(session);
                    start_recording(app, state, ActiveMode::Edit);
                }
            }
            ShortcutState::Released => {
                let session = state.session.lock().unwrap();
                if session.active_mode == ActiveMode::Edit {
                    drop(session);
                    stop_recording(app, state);
                }
            }
        }
        return;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(handle_shortcut)
                .build(),
        )
        .manage(AppState {
            config: Mutex::new(config::Config::load()),
            session: Mutex::new(SessionState {
                active_mode: ActiveMode::Idle,
                recorder: None,
            }),
            parsed_shortcut: Mutex::new(None),
            parsed_edit_shortcut: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            test_dashscope_config,
            test_llm_config,
            get_history,
            delete_history_item,
            clear_history,
            export_history,
            get_stats,
        ])
        .setup(|app| {
            // 设置系统托盘
            if let Err(e) = setup_tray(app) {
                eprintln!("设置系统托盘失败: {}", e);
            }

            // 主窗口关闭时隐藏到托盘，而不是退出
            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        api.prevent_close();
                    }
                });
            }

            // 解析并注册全局快捷键
            let state: State<AppState> = app.state();
            let config = state.config.lock().unwrap().clone();

            let parsed_shortcut = parse_shortcut(&config.shortcut)
                .expect("默认录音快捷键解析失败");
            let parsed_edit_shortcut = parse_shortcut(&config.edit_shortcut)
                .expect("默认编辑快捷键解析失败");

            *state.parsed_shortcut.lock().unwrap() = Some(parsed_shortcut);
            *state.parsed_edit_shortcut.lock().unwrap() = Some(parsed_edit_shortcut);

            if let Err(e) = register_shortcuts(
                app.handle(),
                &parsed_shortcut,
                &parsed_edit_shortcut,
            ) {
                eprintln!("注册全局快捷键失败: {}", e);
                emit_error(app.handle(), "shortcut_register_failed", e);
            }

            // 注册 Esc 取消快捷键
            let esc_shortcut = Shortcut::new(None, Code::Escape);
            if let Err(e) = app.global_shortcut().register(esc_shortcut) {
                eprintln!("注册 Esc 快捷键失败: {}", e);
            }

            // 首次启动显示主窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shortcut_command_period() {
        let shortcut = parse_shortcut("Command+Period").unwrap();
        assert_eq!(shortcut.key, Code::Period);
        assert!(shortcut.mods.contains(Modifiers::SUPER));
    }

    #[test]
    fn test_parse_shortcut_command_option_period() {
        let shortcut = parse_shortcut("Command+Option+Period").unwrap();
        assert_eq!(shortcut.key, Code::Period);
        assert!(shortcut.mods.contains(Modifiers::SUPER));
        assert!(shortcut.mods.contains(Modifiers::ALT));
    }

    #[test]
    fn test_parse_shortcut_aliases() {
        let a = parse_shortcut("Cmd+Alt+.").unwrap();
        let b = parse_shortcut("Command+Option+Period").unwrap();
        assert_eq!(a.key, b.key);
        assert_eq!(a.mods, b.mods);
    }

    #[test]
    fn test_parse_shortcut_bare_escape() {
        let shortcut = parse_shortcut("Escape").unwrap();
        assert!(is_bare_escape(&shortcut));
    }

    #[test]
    fn test_parse_shortcut_empty_rejected() {
        assert!(parse_shortcut("").is_err());
        assert!(parse_shortcut("   ").is_err());
    }

    #[test]
    fn test_parse_shortcut_invalid_modifier() {
        assert!(parse_shortcut("Foo+Period").is_err());
    }

    #[test]
    fn test_parse_shortcut_invalid_key() {
        assert!(parse_shortcut("Command+Foo").is_err());
    }
}
