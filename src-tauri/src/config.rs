use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use crate::dashscope::DashScopeConfig;
pub use crate::dictionary::DictionaryEntry;
pub use crate::llm::LlmConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub shortcut: String,
    pub sound_feedback: bool,
    pub dashscope: DashScopeConfig,
    pub llm: LlmConfig,
    pub output_mode: String, // "polish" | "verbatim"
    pub auto_enter: bool,
    pub recording_mode: String,              // "push_to_talk" | "continuous"
    pub use_clipboard_fallback: bool,        // 长文本或输入失败时改用剪贴板粘贴
    pub clipboard_fallback_threshold: usize, // 触发剪贴板回退的字数阈值

    // V3 新增
    #[serde(default = "default_edit_shortcut")]
    pub edit_shortcut: String, // 默认 "Command+Option+Period"
    #[serde(default)]
    pub dictionary: Vec<DictionaryEntry>,
    #[serde(default = "default_config_version")]
    pub config_version: u32,
}

fn default_edit_shortcut() -> String {
    "Command+Option+Period".to_string()
}

fn default_config_version() -> u32 {
    1
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: "Command+Period".to_string(),
            sound_feedback: true,
            dashscope: DashScopeConfig::default(),
            llm: LlmConfig::default(),
            output_mode: "polish".to_string(),
            auto_enter: false,
            recording_mode: "push_to_talk".to_string(),
            use_clipboard_fallback: true,
            clipboard_fallback_threshold: 100,
            edit_shortcut: "Command+Option+Period".to_string(),
            dictionary: Vec::new(),
            config_version: 1,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<Config>(&content) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("配置文件解析失败: {}, 使用默认配置", e),
                },
                Err(e) => eprintln!("读取配置文件失败: {}, 使用默认配置", e),
            }
        }
        let config = Config::default();
        let _ = config.save();
        config
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notype-mini")
        .join("config.toml")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert_eq!(config.shortcut, "Command+Period");
        assert!(config.sound_feedback);
        assert_eq!(config.output_mode, "polish");
        assert!(!config.auto_enter);
        assert_eq!(config.recording_mode, "push_to_talk");
        assert!(config.use_clipboard_fallback);
        assert_eq!(config.clipboard_fallback_threshold, 100);
        assert_eq!(config.edit_shortcut, "Command+Option+Period");
        assert!(config.dictionary.is_empty());
        assert_eq!(config.config_version, 1);
        assert_eq!(config.dashscope.model, "paraformer-v2");
        assert_eq!(config.llm.model, "deepseek-chat");
    }

    #[test]
    fn test_config_toml_roundtrip() {
        let config = Config::default();
        let toml = toml::to_string_pretty(&config).expect("序列化失败");
        let parsed: Config = toml::from_str(&toml).expect("反序列化失败");
        assert_eq!(parsed.shortcut, config.shortcut);
        assert_eq!(parsed.output_mode, config.output_mode);
        assert_eq!(parsed.dashscope.model, config.dashscope.model);
        assert_eq!(parsed.llm.model, config.llm.model);
        assert_eq!(parsed.edit_shortcut, config.edit_shortcut);
        assert_eq!(parsed.config_version, config.config_version);
    }

    #[test]
    fn test_config_backward_compatible() {
        // 模拟旧版 config.toml（没有 edit_shortcut/dictionary/config_version）
        let old_toml = r#"
shortcut = "Command+Period"
sound_feedback = true
output_mode = "polish"
auto_enter = false
recording_mode = "push_to_talk"
use_clipboard_fallback = true
clipboard_fallback_threshold = 100

[dashscope]
base_url = "https://dashscope.aliyuncs.com"
api_key = ""
model = "paraformer-v2"

[llm]
base_url = "https://api.deepseek.com"
api_key = ""
model = "deepseek-chat"
"#;
        let parsed: Config = toml::from_str(old_toml).expect("旧配置应能解析");
        assert_eq!(parsed.edit_shortcut, "Command+Option+Period");
        assert!(parsed.dictionary.is_empty());
        assert_eq!(parsed.config_version, 1);
    }
}
