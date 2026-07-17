use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_CANDIDATES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateEntry {
    pub from: String,
    pub to: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RejectedPair {
    from: String,
    to: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CandidateStore {
    #[serde(default)]
    pending: Vec<CandidateEntry>,
    #[serde(default)]
    rejected: Vec<RejectedPair>,
}

fn candidates_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notype-mini")
        .join("candidates.json")
}

fn load_store() -> CandidateStore {
    let path = candidates_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<CandidateStore>(&content) {
                Ok(store) => return store,
                Err(e) => eprintln!("候选队列解析失败: {}, 使用空队列", e),
            },
            Err(e) => eprintln!("读取候选队列失败: {}, 使用空队列", e),
        }
    }
    CandidateStore::default()
}

fn save_store(store: &CandidateStore) -> Result<(), Box<dyn std::error::Error>> {
    let path = candidates_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn normalize_pair(from: &str, to: &str) -> (String, String) {
    (from.trim().to_string(), to.trim().to_string())
}

/// 判断两个 from 是否在词典匹配语义下等价（ASCII 不区分大小写，其他精确匹配）
fn from_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.chars()
        .zip(b.chars())
        .all(|(ca, cb)| {
            if ca.is_ascii_alphabetic() && cb.is_ascii_alphabetic() {
                ca.to_ascii_lowercase() == cb.to_ascii_lowercase()
            } else {
                ca == cb
            }
        })
}

/// 把提取出的候选对加入队列，按规则过滤。
pub fn add_candidates(
    pairs: &[(String, String)],
    existing_dictionary: &[(String, String)],
) -> Result<Vec<CandidateEntry>, Box<dyn std::error::Error>> {
    let mut store = load_store();

    for (from, to) in pairs {
        let (from, to) = normalize_pair(from, to);

        // 跳过空 from、无意义对
        if from.is_empty() || from == to {
            continue;
        }

        // 跳过已在现有词典中的对
        if existing_dictionary
            .iter()
            .any(|(ef, et)| from_eq(&from, ef) && to == *et)
        {
            continue;
        }

        // 跳过已拒绝的对
        if store
            .rejected
            .iter()
            .any(|r| from_eq(&from, &r.from) && to == r.to)
        {
            continue;
        }

        // 更新或新增
        if let Some(existing) = store.pending.iter_mut().find(|c| {
            from_eq(&from, &c.from) && to == c.to
        }) {
            existing.count = existing.count.saturating_add(1);
        } else {
            store.pending.push(CandidateEntry { from, to, count: 1 });
        }
    }

    // 限制队列长度，保留 count 最高的
    if store.pending.len() > MAX_CANDIDATES {
        store
            .pending
            .sort_by(|a, b| b.count.cmp(&a.count).then(a.from.cmp(&b.from)));
        store.pending.truncate(MAX_CANDIDATES);
    }

    save_store(&store)?;
    Ok(store.pending)
}

pub fn get_candidates() -> Vec<CandidateEntry> {
    load_store().pending
}

pub fn accept_candidate(from: &str, to: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let mut store = load_store();
    let idx = store
        .pending
        .iter()
        .position(|c| from_eq(from, &c.from) && to == c.to)
        .ok_or("候选不存在")?;
    let entry = store.pending.remove(idx);
    save_store(&store)?;
    Ok((entry.from, entry.to))
}

pub fn reject_candidate(from: &str, to: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_store();
    let idx = store
        .pending
        .iter()
        .position(|c| from_eq(from, &c.from) && to == c.to)
        .ok_or("候选不存在")?;
    store.pending.remove(idx);

    // 避免 rejected 里重复
    if !store
        .rejected
        .iter()
        .any(|r| from_eq(from, &r.from) && to == r.to)
    {
        store.rejected.push(RejectedPair {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    save_store(&store)?;
    Ok(())
}

/// 字符级 diff，提取 (original_fragment, corrected_fragment) 替换对。
/// 使用 `TextDiff::ops()` 直接获取 DiffOp，其中 `Replace` op 已经把相邻的
/// delete + insert 合并为一次替换；`Delete` 作为 (deleted, ""); `Insert` 跳过。
pub fn extract_correction_pairs(original: &str, corrected: &str) -> Vec<(String, String)> {
    use similar::{DiffOp, TextDiff};

    let diff = TextDiff::from_chars(original, corrected);
    let ops = diff.ops();

    let old_chars: Vec<char> = original.chars().collect();
    let new_chars: Vec<char> = corrected.chars().collect();

    let mut pairs = Vec::new();
    for op in ops {
        match *op {
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let from: String = old_chars[old_index..old_index + old_len].iter().collect();
                let to: String = new_chars[new_index..new_index + new_len].iter().collect();
                pairs.push((from, to));
            }
            DiffOp::Delete {
                old_index,
                old_len,
                ..
            } => {
                let from: String = old_chars[old_index..old_index + old_len].iter().collect();
                pairs.push((from, String::new()));
            }
            DiffOp::Insert { .. } => {
                // 纯新增没有 from，无法作为词典候选
            }
            DiffOp::Equal { .. } => {}
        }
    }

    pairs
}

#[cfg(test)]
pub fn clear_test_store() {
    let path = candidates_path();
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset() {
        clear_test_store();
    }

    fn run<F: FnOnce()>(f: F) {
        let _guard = TEST_LOCK.lock().unwrap();
        reset();
        f();
        reset();
    }

    #[test]
    fn test_extract_single_replacement() {
        run(|| {
            let pairs = extract_correction_pairs("拉斯特很好用", "Rust很好用");
            assert_eq!(pairs, vec![("拉斯特".to_string(), "Rust".to_string())]);
        });
    }

    #[test]
    fn test_extract_multiple_replacements() {
        run(|| {
            // 注意：LCS diff 会尽量保留公共字符。"一鸣" → "依鸣" 中"鸣"相同，
            // 所以只产生 "一" → "依"，而不是 "一鸣" → "依鸣"。这是字符级 diff 的预期行为。
            let pairs = extract_correction_pairs("拉斯特和一鸣", "Rust和依鸣");
            assert_eq!(
                pairs,
                vec![
                    ("拉斯特".to_string(), "Rust".to_string()),
                    ("一".to_string(), "依".to_string()),
                ]
            );
        });
    }

    #[test]
    fn test_extract_pure_delete() {
        run(|| {
            let pairs = extract_correction_pairs("嗯好的", "好的");
            assert_eq!(pairs, vec![("嗯".to_string(), "".to_string())]);
        });
    }

    #[test]
    fn test_extract_pure_insert_is_skipped() {
        run(|| {
            let pairs = extract_correction_pairs("好的", "嗯好的");
            assert!(pairs.is_empty());
        });
    }

    #[test]
    fn test_extract_no_change() {
        run(|| {
            let pairs = extract_correction_pairs("完全一样", "完全一样");
            assert!(pairs.is_empty());
        });
    }

    #[test]
    fn test_add_candidates_filters_existing_and_rejected() {
        run(|| {
            let dict = vec![("拉斯特".to_string(), "Rust".to_string())];
            let pairs = vec![
                ("拉斯特".to_string(), "Rust".to_string()), // 已存在，跳过
                ("一鸣".to_string(), "依鸣".to_string()),   // 实际产生 "一鸣" → "依鸣"
                ("  一鸣  ".to_string(), "依鸣".to_string()), // trim 后同样为 "一鸣" → "依鸣"
                ("那个".to_string(), "".to_string()),       // 纯删除，允许
                ("".to_string(), "x".to_string()),          // 空 from，跳过
                ("相同".to_string(), "相同".to_string()),   // from==to，跳过
            ];

            let pending = add_candidates(&pairs, &dict).unwrap();
            assert_eq!(pending.len(), 2);

            let yiming = pending.iter().find(|c| c.from == "一鸣").unwrap();
            assert_eq!(yiming.count, 2);
            assert_eq!(yiming.to, "依鸣");

            let nage = pending.iter().find(|c| c.from == "那个").unwrap();
            assert_eq!(nage.to, "");
            assert_eq!(nage.count, 1);
        });
    }

    #[test]
    fn test_reject_and_re_add_is_ignored() {
        run(|| {
            let pairs = vec![("一鸣".to_string(), "依鸣".to_string())];
            let pending = add_candidates(&pairs, &[]).unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].from, "一鸣");
            assert_eq!(pending[0].to, "依鸣");

            reject_candidate("一鸣", "依鸣").unwrap();

            let pending2 = add_candidates(&pairs, &[]).unwrap();
            assert!(pending2.is_empty());
        });
    }

    #[test]
    fn test_accept_candidate() {
        run(|| {
            let pairs = vec![("一鸣".to_string(), "依鸣".to_string())];
            add_candidates(&pairs, &[]).unwrap();

            let (from, to) = accept_candidate("一鸣", "依鸣").unwrap();
            assert_eq!(from, "一鸣");
            assert_eq!(to, "依鸣");

            assert!(get_candidates().is_empty());
        });
    }
}
