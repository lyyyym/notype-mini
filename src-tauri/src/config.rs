use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use crate::dashscope::DashScopeConfig;
pub use crate::llm::LlmConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub shortcut: String,
    pub sound_feedback: bool,
    pub dashscope: DashScopeConfig,
    pub llm: LlmConfig,
    pub output_mode: String, // "polish" | "verbatim"
    pub auto_enter: bool,
    pub recording_mode: String,             // "push_to_talk" | "continuous"
    pub use_clipboard_fallback: bool,       // 长文本或输入失败时改用剪贴板粘贴
    pub clipboard_fallback_threshold: usize, // 触发剪贴板回退的字数阈值
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
    }
}
