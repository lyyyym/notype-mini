use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_HISTORY_ENTRIES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub raw_text: String,
    pub polished_text: String,
    pub word_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryStore {
    entries: Vec<HistoryEntry>,
}

fn history_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notype-mini")
        .join("history.json")
}

fn load_store() -> HistoryStore {
    let path = history_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<HistoryStore>(&content) {
                Ok(store) => return store,
                Err(e) => eprintln!("历史记录解析失败: {}, 使用空记录", e),
            },
            Err(e) => eprintln!("读取历史记录失败: {}, 使用空记录", e),
        }
    }
    HistoryStore::default()
}

fn save_store(store: &HistoryStore) -> Result<(), Box<dyn std::error::Error>> {
    let path = history_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn add_entry(entry: HistoryEntry) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_store();
    store.entries.insert(0, entry);
    if store.entries.len() > MAX_HISTORY_ENTRIES {
        store.entries.truncate(MAX_HISTORY_ENTRIES);
    }
    save_store(&store)
}

pub fn get_entries(limit: Option<usize>) -> Vec<HistoryEntry> {
    let store = load_store();
    match limit {
        Some(n) => store.entries.into_iter().take(n).collect(),
        None => store.entries,
    }
}

pub fn delete_entry(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_store();
    store.entries.retain(|e| e.id != id);
    save_store(&store)
}

pub fn clear_history() -> Result<(), Box<dyn std::error::Error>> {
    save_store(&HistoryStore::default())
}

pub fn export_to_markdown() -> Result<String, Box<dyn std::error::Error>> {
    let store = load_store();
    let mut md = String::from("# NoType Mini 转写历史\n\n");

    for entry in store.entries {
        let time = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        md.push_str(&format!("## {}\n\n{}\n\n", time, entry.polished_text));
    }

    Ok(md)
}

pub fn get_stats() -> HistoryStats {
    let store = load_store();
    let total_words: usize = store.entries.iter().map(|e| e.word_count).sum();
    let total_sessions = store.entries.len();

    let today = Local::now().date_naive();
    let today_words: usize = store
        .entries
        .iter()
        .filter(|e| e.timestamp.date_naive() == today)
        .map(|e| e.word_count)
        .sum();

    HistoryStats {
        total_words,
        today_words,
        total_sessions,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryStats {
    pub total_words: usize,
    pub today_words: usize,
    pub total_sessions: usize,
}
