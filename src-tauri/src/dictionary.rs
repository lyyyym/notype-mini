use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DictionaryEntry {
    pub from: String,
    pub to: String,
}

/// 清洗并验证词典条目。
/// - from/to 会被 trim
/// - 跳过 from 为空的条目
/// - 拒绝 from 重复的条目
pub fn validate_dictionary(entries: &[DictionaryEntry]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();

    for entry in entries {
        let from = entry.from.trim();
        let to = entry.to.trim();

        if from.is_empty() {
            continue;
        }

        let normalized = from.to_lowercase();
        if !seen.insert(normalized) {
            return Err(format!("词典中存在重复的 'from' 条目: {}", from));
        }

        // to 为空是允许的（相当于删除该词）
        let _ = to;
    }

    Ok(())
}

/// 对文本应用词典替换。
///
/// 算法：从文本开头扫描到结尾，在每一位置尝试所有 entry，选择能匹配的最长 from。
/// 如果找到匹配，替换为对应 to，光标前进 from 的长度；否则保留当前字符，光标前进 1。
/// ASCII 条目不区分大小写；中文按字符边界匹配。
/// 该算法保证非重叠、最长匹配、避免级联替换。
pub fn apply_dictionary(text: &str, entries: &[DictionaryEntry]) -> String {
    // 清洗：过滤掉 from 为空的条目，并 trim
    let cleaned: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|e| {
            let from = e.from.trim();
            let to = e.to.trim();
            if from.is_empty() {
                None
            } else {
                Some((from, to))
            }
        })
        .collect();

    if cleaned.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let mut best_len = 0usize;
        let mut best_to = "";

        for (from, to) in &cleaned {
            let from_chars: Vec<char> = from.chars().collect();
            if i + from_chars.len() > chars.len() {
                continue;
            }

            let slice = &chars[i..i + from_chars.len()];
            if slice_equal_case_insensitive(slice, &from_chars) {
                if from_chars.len() > best_len {
                    best_len = from_chars.len();
                    best_to = *to;
                }
            }
        }

        if best_len > 0 {
            result.push_str(best_to);
            i += best_len;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

fn slice_equal_case_insensitive(a: &[char], b: &[char]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    a.iter().zip(b.iter()).all(|(ca, cb)| {
        // 如果是 ASCII 字母，不区分大小写；其他字符精确匹配
        if ca.is_ascii_alphabetic() && cb.is_ascii_alphabetic() {
            ca.to_ascii_lowercase() == cb.to_ascii_lowercase()
        } else {
            ca == cb
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(list: &[(&str, &str)]) -> Vec<DictionaryEntry> {
        list.iter()
            .map(|(from, to)| DictionaryEntry {
                from: from.to_string(),
                to: to.to_string(),
            })
            .collect()
    }

    #[test]
    fn test_basic_replacement() {
        let entries = entries(&[("拉斯特", "Rust"), ("嗯", "")]
        );
        assert_eq!(
            apply_dictionary("拉斯特很好用", &entries),
            "Rust很好用"
        );
    }

    #[test]
    fn test_longest_match() {
        let entries = entries(&[("拉斯特", "Rust"), ("拉斯特级", "RustLevel")]
        );
        // "拉斯特级" 比 "拉斯特" 长，应匹配 "拉斯特级"
        assert_eq!(
            apply_dictionary("拉斯特级战舰", &entries),
            "RustLevel战舰"
        );
    }

    #[test]
    fn test_no_cascade() {
        let entries = entries(&[("a", "b"), ("b", "c")]
        );
        // 不应出现 a -> b -> c 的级联
        assert_eq!(apply_dictionary("a", &entries), "b");
        assert_eq!(apply_dictionary("b", &entries), "c");
    }

    #[test]
    fn test_case_insensitive_ascii() {
        let entries = entries(&[("rust", "Rust")]
        );
        assert_eq!(apply_dictionary("RUST is great", &entries), "Rust is great");
        assert_eq!(apply_dictionary("Rust is great", &entries), "Rust is great");
    }

    #[test]
    fn test_trim_and_empty() {
        let entries = vec![
            DictionaryEntry {
                from: "  拉斯特  ".to_string(),
                to: "  Rust  ".to_string(),
            },
            DictionaryEntry {
                from: "   ".to_string(),
                to: "x".to_string(),
            },
        ];
        assert_eq!(apply_dictionary("拉斯特很好用", &entries), "Rust很好用");
    }

    #[test]
    fn test_validation_rejects_duplicates() {
        let entries = vec![
            DictionaryEntry {
                from: "拉斯特".to_string(),
                to: "Rust".to_string(),
            },
            DictionaryEntry {
                from: "拉斯特".to_string(),
                to: "Rust2".to_string(),
            },
        ];
        assert!(validate_dictionary(&entries).is_err());
    }

    #[test]
    fn test_validation_skips_empty() {
        let entries = vec![
            DictionaryEntry {
                from: "".to_string(),
                to: "x".to_string(),
            },
            DictionaryEntry {
                from: "拉斯特".to_string(),
                to: "Rust".to_string(),
            },
        ];
        assert!(validate_dictionary(&entries).is_ok());
    }
}
