mod audio;
mod config;
mod dashscope;
mod history;
mod llm;

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

// 事件载荷结构
#[derive(Clone, serde::Serialize)]
struct RecordingStateEvent {
    state: String,
}

#[derive(Clone, serde::Serialize)]
struct TranscriptionEvent {
    text: String,
}

#[derive(Clone, serde::Serialize)]
struct ErrorEvent {
    code: String,
    message: String,
}

// 应用状态
struct AppState {
    config: Mutex<config::Config>,
    recorder: Mutex<Option<audio::RecorderHandle>>,
}

// 命令：获取配置
#[tauri::command]
fn get_config(state: State<AppState>) -> config::Config {
    state.config.lock().unwrap().clone()
}

// 命令：保存配置
#[tauri::command]
fn set_config(new_config: config::Config, state: State<AppState>) -> Result<(), String> {
    new_config.save().map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = new_config;
    Ok(())
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
fn emit_state(app: &AppHandle, state: &str) {
    let _ = app.emit("recording-state", RecordingStateEvent {
        state: state.to_string(),
    });
}

fn emit_result(app: &AppHandle, text: String) {
    let _ = app.emit("transcription-result", TranscriptionEvent { text });
}

fn emit_error(app: &AppHandle, code: &str, message: String) {
    let _ = app.emit("error", ErrorEvent {
        code: code.to_string(),
        message,
    });
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

// 开始录音
fn start_recording(app: &AppHandle, state: State<AppState>) {
    let app_handle = app.clone();

    match audio::start_recording(app_handle) {
        Ok(handle) => {
            *state.recorder.lock().unwrap() = Some(handle);
            show_bubble(app);
            emit_state(app, "recording");
        }
        Err(e) => {
            eprintln!("开始录音失败: {}", e);
            emit_error(app, "recording_start_failed", e.to_string());
            emit_state(app, "idle");
        }
    }
}

// 文字输入到光标位置
fn type_text(text: &str, auto_enter: bool) -> Result<(), anyhow::Error> {
    use enigo::{Direction::Click, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())?;

    // 模拟键盘输入文字
    enigo.text(text).map_err(|e| anyhow::anyhow!("键盘输入失败: {:?}", e))?;

    if auto_enter {
        enigo.key(Key::Return, Click).map_err(|e| anyhow::anyhow!("回车失败: {:?}", e))?;
    }

    Ok(())
}

// 停止录音并处理识别
fn stop_recording(app: &AppHandle, state: State<AppState>) {
    // 取出 recorder
    let handle = {
        let mut guard = state.recorder.lock().unwrap();
        guard.take()
    };

    if let Some(handle) = handle {
        hide_bubble(app);
        emit_state(app, "processing");

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
                        emit_state(&app_clone, "idle");
                        return;
                    }

                    match audio::samples_to_wav(&samples, sample_rate) {
                        Ok(wav_bytes) => {
                            match dashscope::transcribe(wav_bytes, &config.dashscope).await {
                                Ok(raw_text) => {
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
                                    if let Err(e) = type_text(&final_text, config.auto_enter) {
                                        eprintln!("输入失败: {}", e);
                                        emit_error(
                                            &app_clone,
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
                                        duration_ms: (samples.len() as f64 / sample_rate as f64 * 1000.0) as u64,
                                    };
                                    if let Err(e) = history::add_entry(entry) {
                                        eprintln!("保存历史记录失败: {}", e);
                                    }

                                    emit_result(&app_clone, final_text);
                                    emit_state(&app_clone, "idle");
                                }
                                Err(e) => {
                                    eprintln!("识别失败: {}", e);
                                    emit_error(
                                        &app_clone,
                                        "transcription_failed",
                                        e.to_string(),
                                    );
                                    emit_state(&app_clone, "idle");
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
                            emit_state(&app_clone, "idle");
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
                    emit_state(&app_clone, "idle");
                }
            }
        });
    }
}

// 全局快捷键处理函数
fn handle_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    let state: State<AppState> = app.state();

    match event.state() {
        ShortcutState::Pressed => {
            // 避免重复开始
            if state.recorder.lock().unwrap().is_none() {
                start_recording(app, state);
            }
        }
        ShortcutState::Released => {
            if state.recorder.lock().unwrap().is_some() {
                stop_recording(app, state);
            }
        }
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
            recorder: Mutex::new(None),
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
            // 注册全局快捷键
            let shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::Period);
            if let Err(e) = app.global_shortcut().register(shortcut) {
                eprintln!("注册全局快捷键失败: {}", e);
                emit_error(app.handle(), "shortcut_register_failed", e.to_string());
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
