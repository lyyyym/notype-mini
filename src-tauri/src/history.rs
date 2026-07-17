use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_HISTORY_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Transcribe,
    Edit,
}

impl Default for EntryType {
    fn default() -> Self {
        EntryType::Transcribe
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub raw_text: String,
    pub polished_text: String,
    pub word_count: usize,
    pub duration_ms: u64,
    #[serde(default)]
    pub entry_type: EntryType,
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

pub fn update_polished_text(
    id: &str,
    new_text: String,
) -> Result<Option<HistoryEntry>, Box<dyn std::error::Error>> {
    let mut store = load_store();
    let idx = store.entries.iter().position(|e| e.id == id);
    if let Some(idx) = idx {
        store.entries[idx].polished_text = new_text;
        store.entries[idx].word_count = store.entries[idx].polished_text.chars().count();
        let entry = store.entries[idx].clone();
        save_store(&store)?;
        Ok(Some(entry))
    } else {
        Ok(None)
    }
}

pub fn export_to_markdown() -> Result<String, Box<dyn std::error::Error>> {
    let store = load_store();
    let mut md = String::from("# NoType Mini 转写历史\n\n");

    for entry in store.entries {
        let time = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let type_label = match entry.entry_type {
            EntryType::Transcribe => "录音",
            EntryType::Edit => "编辑",
        };
        md.push_str(&format!(
            "## [{}] {}\n\n{}\n\n",
            type_label, time, entry.polished_text
        ));
    }

    Ok(md)
}

pub fn get_stats() -> HistoryStats {
    let store = load_store();
    let today = Local::now().date_naive();

    let (transcribe_total_words, transcribe_today_words, transcribe_total_sessions) =
        store.entries.iter().fold((0, 0, 0), |acc, e| {
            if e.entry_type == EntryType::Transcribe {
                (
                    acc.0 + e.word_count,
                    if e.timestamp.date_naive() == today {
                        acc.1 + e.word_count
                    } else {
                        acc.1
                    },
                    acc.2 + 1,
                )
            } else {
                acc
            }
        });

    let (edit_total_words, edit_today_words, edit_total_sessions) =
        store.entries.iter().fold((0, 0, 0), |acc, e| {
            if e.entry_type == EntryType::Edit {
                (
                    acc.0 + e.word_count,
                    if e.timestamp.date_naive() == today {
                        acc.1 + e.word_count
                    } else {
                        acc.1
                    },
                    acc.2 + 1,
                )
            } else {
                acc
            }
        });

    HistoryStats {
        transcribe_total_words,
        transcribe_today_words,
        transcribe_total_sessions,
        edit_total_words,
        edit_today_words,
        edit_total_sessions,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryStats {
    pub transcribe_total_words: usize,
    pub transcribe_today_words: usize,
    pub transcribe_total_sessions: usize,
    pub edit_total_words: usize,
    pub edit_today_words: usize,
    pub edit_total_sessions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_split_by_type() {
        let entries = vec![
            HistoryEntry {
                id: "1".to_string(),
                timestamp: Local::now(),
                raw_text: "raw".to_string(),
                polished_text: "hello world".to_string(),
                word_count: 2,
                duration_ms: 1000,
                entry_type: EntryType::Transcribe,
            },
            HistoryEntry {
                id: "2".to_string(),
                timestamp: Local::now(),
                raw_text: "raw".to_string(),
                polished_text: "foo bar baz".to_string(),
                word_count: 3,
                duration_ms: 1000,
                entry_type: EntryType::Edit,
            },
        ];

        let mut store = HistoryStore::default();
        store.entries = entries;
        // 直接保存再加载来测试 stats
        let _ = save_store(&store);
        let stats = get_stats();
        assert_eq!(stats.transcribe_total_words, 2);
        assert_eq!(stats.edit_total_words, 3);
        assert_eq!(stats.transcribe_total_sessions, 1);
        assert_eq!(stats.edit_total_sessions, 1);
    }
}
