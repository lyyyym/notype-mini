use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use base64::Engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashScopeConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for DashScopeConfig {
    fn default() -> Self {
        Self {
            base_url: "https://dashscope.aliyuncs.com".to_string(),
            api_key: String::new(),
            model: "paraformer-v2".to_string(),
        }
    }
}

impl DashScopeConfig {
    /// 返回去掉尾部斜杠和 /compatible-mode/v1 的 base_url
    fn base(&self) -> String {
        self.base_url
            .trim_end_matches('/')
            .trim_end_matches("/compatible-mode/v1")
            .trim_end_matches('/')
            .to_string()
    }

    fn files_url(&self) -> String {
        format!("{}/api/v1/files", self.base())
    }

    fn file_detail_url(&self, file_id: &str) -> String {
        format!("{}/api/v1/files/{}", self.base(), file_id)
    }

    fn paraformer_transcription_url(&self) -> String {
        format!("{}/api/v1/services/audio/asr/transcription", self.base())
    }

    fn task_url(&self, task_id: &str) -> String {
        format!("{}/api/v1/tasks/{}", self.base(), task_id)
    }

    fn qwen_asr_url(&self) -> String {
        format!("{}/api/v1/services/aigc/multimodal-generation/generation", self.base())
    }

    fn models_url(&self) -> String {
        format!("{}/compatible-mode/v1/models", self.base())
    }

    fn is_qwen_asr(&self) -> bool {
        let m = self.model.to_lowercase();
        m.contains("qwen") && m.contains("asr")
    }
}

// ==================== Paraformer 异步转写 ====================

#[derive(Debug, Deserialize)]
struct UploadResponse {
    data: UploadData,
}

#[derive(Debug, Deserialize)]
struct UploadData {
    uploaded_files: Vec<UploadedFile>,
}

#[derive(Debug, Deserialize)]
struct UploadedFile {
    file_id: String,
}

#[derive(Debug, Deserialize)]
struct FileDetailResponse {
    data: FileDetail,
}

#[derive(Debug, Deserialize)]
struct FileDetail {
    url: String,
}

#[derive(Debug, Serialize)]
struct ParaformerRequest {
    model: String,
    input: ParaformerInput,
    parameters: ParaformerParameters,
}

#[derive(Debug, Serialize)]
struct ParaformerInput {
    file_urls: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ParaformerParameters {
    language_hints: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TaskResponse {
    output: TaskOutput,
}

#[derive(Debug, Deserialize)]
struct TaskOutput {
    task_id: String,
    task_status: String,
    #[serde(default)]
    results: Vec<TaskResultWrapper>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskResultWrapper {
    output: SubTaskOutput,
}

#[derive(Debug, Deserialize)]
struct SubTaskOutput {
    #[serde(default)]
    results: Vec<Segment>,
}

#[derive(Debug, Deserialize)]
struct Segment {
    #[serde(default)]
    text: String,
}

// ==================== Qwen-ASR 同步识别 ====================

#[derive(Debug, Serialize)]
struct QwenAsrRequest {
    model: String,
    input: QwenAsrInput,
    parameters: QwenAsrParameters,
}

#[derive(Debug, Serialize)]
struct QwenAsrInput {
    messages: Vec<QwenAsrMessage>,
}

#[derive(Debug, Serialize)]
struct QwenAsrMessage {
    role: String,
    content: Vec<QwenAsrContent>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum QwenAsrContent {
    Text { text: String },
    Audio { audio: String },
}

#[derive(Debug, Serialize)]
struct QwenAsrParameters {
    #[serde(rename = "asr_options")]
    asr_options: QwenAsrOptions,
}

#[derive(Debug, Serialize)]
struct QwenAsrOptions {
    enable_itn: bool,
    language: String,
}

#[derive(Debug, Deserialize)]
struct QwenAsrResponse {
    output: QwenAsrOutput,
}

#[derive(Debug, Deserialize)]
struct QwenAsrOutput {
    choices: Vec<QwenAsrChoice>,
}

#[derive(Debug, Deserialize)]
struct QwenAsrChoice {
    message: QwenAsrResponseMessage,
}

#[derive(Debug, Deserialize)]
struct QwenAsrResponseMessage {
    content: Vec<QwenAsrResponseContent>,
}

#[derive(Debug, Deserialize)]
struct QwenAsrResponseContent {
    #[serde(default)]
    text: String,
}

// ==================== 公共入口 ====================

pub async fn transcribe(audio_bytes: Vec<u8>, config: &DashScopeConfig) -> Result<String, anyhow::Error> {
    if config.api_key.is_empty() {
        return Err(anyhow::anyhow!("DashScope API Key 未配置"));
    }

    if config.is_qwen_asr() {
        transcribe_qwen_asr(audio_bytes, config).await
    } else {
        transcribe_paraformer(audio_bytes, config).await
    }
}

// ==================== Paraformer 实现 ====================

async fn transcribe_paraformer(
    audio_bytes: Vec<u8>,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let file_id = upload_file(&client, audio_bytes, config).await?;
    let file_url = get_file_url_with_retry(&client, &file_id, config).await?;
    let task_id = submit_paraformer_task(&client, &file_url, config).await?;
    let text = poll_task(&client, &task_id, config).await?;

    Ok(text)
}

async fn upload_file(
    client: &reqwest::Client,
    audio_bytes: Vec<u8>,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = multipart::Form::new()
        .text("purpose", "assistants")
        .part("file", part);

    let resp = client
        .post(config.files_url())
        .bearer_auth(&config.api_key)
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("文件上传失败 ({}): {}", status, text));
    }

    let result: UploadResponse = resp.json().await?;
    result
        .data
        .uploaded_files
        .first()
        .map(|f| f.file_id.clone())
        .ok_or_else(|| anyhow::anyhow!("文件上传响应中未找到 file_id"))
}

async fn get_file_url_with_retry(
    client: &reqwest::Client,
    file_id: &str,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let max_retries = 5;
    let mut attempt = 0;

    loop {
        match get_file_url(client, file_id, config).await {
            Ok(url) => return Ok(url),
            Err(e) => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(e);
                }
                let err_msg = e.to_string();
                if err_msg.contains("429") {
                    sleep(Duration::from_millis(500 * attempt as u64)).await;
                    continue;
                }
                return Err(e);
            }
        }
    }
}

async fn get_file_url(
    client: &reqwest::Client,
    file_id: &str,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let url = config.file_detail_url(file_id);
    let resp = client
        .get(&url)
        .bearer_auth(&config.api_key)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("获取文件 URL 失败 ({}): {}", status, text));
    }

    let result: FileDetailResponse = resp.json().await?;
    Ok(result.data.url)
}

async fn submit_paraformer_task(
    client: &reqwest::Client,
    file_url: &str,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let request = ParaformerRequest {
        model: config.model.clone(),
        input: ParaformerInput {
            file_urls: vec![file_url.to_string()],
        },
        parameters: ParaformerParameters {
            language_hints: vec!["zh".to_string()],
        },
    };

    let resp = client
        .post(config.paraformer_transcription_url())
        .bearer_auth(&config.api_key)
        .header("X-DashScope-Async", "enable")
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("提交转写任务失败 ({}): {}", status, text));
    }

    let result: TaskResponse = resp.json().await?;
    Ok(result.output.task_id)
}

async fn poll_task(
    client: &reqwest::Client,
    task_id: &str,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let url = config.task_url(task_id);
    let max_attempts = 60;
    let mut attempts = 0;

    loop {
        if attempts >= max_attempts {
            return Err(anyhow::anyhow!("转写任务超时，请稍后重试"));
        }
        attempts += 1;

        let resp = client
            .get(&url)
            .bearer_auth(&config.api_key)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("查询任务失败 ({}): {}", status, text));
        }

        let result: TaskResponse = resp.json().await?;

        match result.output.task_status.as_str() {
            "SUCCEEDED" => {
                let mut texts = Vec::new();
                for wrapper in result.output.results {
                    for segment in wrapper.output.results {
                        if !segment.text.is_empty() {
                            texts.push(segment.text);
                        }
                    }
                }
                return Ok(texts.join(""));
            }
            "FAILED" => {
                let message = result
                    .output
                    .message
                    .unwrap_or_else(|| "转写任务失败".to_string());
                return Err(anyhow::anyhow!("{}", message));
            }
            _ => {
                sleep(Duration::from_millis(800)).await;
            }
        }
    }
}

// ==================== Qwen-ASR 实现 ====================

async fn transcribe_qwen_asr(
    audio_bytes: Vec<u8>,
    config: &DashScopeConfig,
) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let base64_audio = format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&audio_bytes)
    );

    let request = QwenAsrRequest {
        model: config.model.clone(),
        input: QwenAsrInput {
            messages: vec![
                QwenAsrMessage {
                    role: "system".to_string(),
                    content: vec![QwenAsrContent::Text { text: "".to_string() }],
                },
                QwenAsrMessage {
                    role: "user".to_string(),
                    content: vec![QwenAsrContent::Audio { audio: base64_audio }],
                },
            ],
        },
        parameters: QwenAsrParameters {
            asr_options: QwenAsrOptions {
                enable_itn: false,
                language: "zh".to_string(),
            },
        },
    };

    let resp = client
        .post(config.qwen_asr_url())
        .bearer_auth(&config.api_key)
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Qwen-ASR 识别失败 ({}): {}", status, text));
    }

    let result: QwenAsrResponse = resp.json().await?;
    let text = result
        .output
        .choices
        .first()
        .and_then(|c| c.message.content.first())
        .map(|c| c.text.clone())
        .unwrap_or_default();

    Ok(text)
}

// ==================== 连接测试 ====================

pub async fn test_connection(config: &DashScopeConfig) -> Result<String, anyhow::Error> {
    if config.api_key.is_empty() {
        return Err(anyhow::anyhow!("API Key 为空"));
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(config.models_url())
        .bearer_auth(&config.api_key)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("连接测试失败 ({}): {}", status, text));
    }

    Ok("DashScope API Key 有效".to_string())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_base_normalizes_trailing_slash() {
        let config = DashScopeConfig {
            base_url: "https://dashscope.aliyuncs.com/".to_string(),
            api_key: "test".to_string(),
            model: "paraformer-v2".to_string(),
        };
        assert_eq!(config.base(), "https://dashscope.aliyuncs.com");
        assert_eq!(config.files_url(), "https://dashscope.aliyuncs.com/api/v1/files");
    }

    #[test]
    fn test_url_base_strips_compatible_mode_suffix() {
        let config = DashScopeConfig {
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1/".to_string(),
            api_key: "test".to_string(),
            model: "paraformer-v2".to_string(),
        };
        assert_eq!(config.base(), "https://dashscope.aliyuncs.com");
        assert_eq!(config.models_url(), "https://dashscope.aliyuncs.com/compatible-mode/v1/models");
    }

    #[test]
    fn test_is_qwen_asr_detection() {
        let mut config = DashScopeConfig {
            base_url: "https://dashscope.aliyuncs.com".to_string(),
            api_key: "test".to_string(),
            model: "qwen3-asr-flash".to_string(),
        };
        assert!(config.is_qwen_asr());

        config.model = "paraformer-v2".to_string();
        assert!(!config.is_qwen_asr());
    }
}
