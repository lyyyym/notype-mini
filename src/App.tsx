import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface DashScopeConfig {
  base_url: string;
  api_key: string;
  model: string;
}

interface LlmConfig {
  base_url: string;
  api_key: string;
  model: string;
}

interface Config {
  shortcut: string;
  sound_feedback: boolean;
  dashscope: DashScopeConfig;
  llm: LlmConfig;
  output_mode: string;
  auto_enter: boolean;
  recording_mode: "push_to_talk" | "continuous";
  use_clipboard_fallback: boolean;
  clipboard_fallback_threshold: number;
}

interface HistoryEntry {
  id: string;
  timestamp: string;
  raw_text: string;
  polished_text: string;
  word_count: number;
  duration_ms: number;
}

interface HistoryStats {
  total_words: number;
  today_words: number;
  total_sessions: number;
}

interface TranscriptionEvent {
  text: string;
}

function App() {
  const [config, setConfig] = useState<Config>({
    shortcut: "Command+Period",
    sound_feedback: true,
    dashscope: {
      base_url: "https://dashscope.aliyuncs.com",
      api_key: "",
      model: "paraformer-v2",
    },
    llm: {
      base_url: "https://api.deepseek.com",
      api_key: "",
      model: "deepseek-chat",
    },
    output_mode: "polish",
    auto_enter: false,
    recording_mode: "push_to_talk",
    use_clipboard_fallback: true,
    clipboard_fallback_threshold: 100,
  });

  const [status, setStatus] = useState("等待开始...");
  const [statusType, setStatusType] = useState<"" | "success" | "error">("");
  const [lastResult, setLastResult] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [stats, setStats] = useState<HistoryStats>({
    total_words: 0,
    today_words: 0,
    total_sessions: 0,
  });

  const loadHistory = async () => {
    try {
      const entries = await invoke<HistoryEntry[]>("get_history", { limit: 50 });
      setHistory(entries);
    } catch (e) {
      console.error("加载历史失败:", e);
    }
  };

  const loadStats = async () => {
    try {
      const s = await invoke<HistoryStats>("get_stats");
      setStats(s);
    } catch (e) {
      console.error("加载统计失败:", e);
    }
  };

  useEffect(() => {
    // 加载配置
    invoke<Config>("get_config").then((loaded) => {
      setConfig(loaded);
    });

    loadHistory();
    loadStats();

    // 监听事件
    const unlistenState = listen<{ state: string }>("recording-state", (event) => {
      if (event.payload.state === "recording") {
        setStatus("正在录音...");
        setStatusType("");
      } else if (event.payload.state === "processing") {
        setStatus("正在识别/整理...");
        setStatusType("");
      } else if (event.payload.state === "idle") {
        setStatus("等待开始...");
      }
    });

    const unlistenResult = listen<TranscriptionEvent>("transcription-result", (event) => {
      setLastResult(event.payload.text);
      setStatus("识别完成并已输入");
      setStatusType("success");
      loadHistory();
      loadStats();
    });

    const unlistenError = listen<{ code: string; message: string }>("error", (event) => {
      setStatus(`错误: ${event.payload.message}`);
      setStatusType("error");
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenResult.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const handleSave = async () => {
    try {
      await invoke("set_config", { newConfig: config });
      setStatus("配置已保存");
      setStatusType("success");
    } catch (e) {
      setStatus(`保存失败: ${e}`);
      setStatusType("error");
    }
  };

  const handleTestDashScope = async () => {
    setStatus("正在测试 DashScope 连接...");
    setStatusType("");
    try {
      const result = await invoke<string>("test_dashscope_config", {
        config: config.dashscope,
      });
      setStatus(result);
      setStatusType("success");
    } catch (e) {
      setStatus(`测试失败: ${e}`);
      setStatusType("error");
    }
  };

  const handleTestLlm = async () => {
    setStatus("正在测试 LLM 连接...");
    setStatusType("");
    try {
      const result = await invoke<string>("test_llm_config", {
        config: config.llm,
      });
      setStatus(result);
      setStatusType("success");
    } catch (e) {
      setStatus(`测试失败: ${e}`);
      setStatusType("error");
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    setStatus("已复制到剪贴板");
    setStatusType("success");
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke("delete_history_item", { id });
      loadHistory();
      loadStats();
    } catch (e) {
      setStatus(`删除失败: ${e}`);
      setStatusType("error");
    }
  };

  const handleClearHistory = async () => {
    if (!confirm("确定要清空所有历史记录吗？此操作不可恢复。")) {
      return;
    }
    try {
      await invoke("clear_history");
      loadHistory();
      loadStats();
      setStatus("历史记录已清空");
      setStatusType("success");
    } catch (e) {
      setStatus(`清空失败: ${e}`);
      setStatusType("error");
    }
  };

  const handleExport = async () => {
    try {
      const markdown = await invoke<string>("export_history");
      const blob = new Blob([markdown], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `notype-mini-history-${new Date().toISOString().slice(0, 10)}.md`;
      a.click();
      URL.revokeObjectURL(url);
      setStatus("历史记录已导出");
      setStatusType("success");
    } catch (e) {
      setStatus(`导出失败: ${e}`);
      setStatusType("error");
    }
  };

  const updateDashScope = (field: keyof DashScopeConfig, value: string) => {
    setConfig((prev) => ({
      ...prev,
      dashscope: { ...prev.dashscope, [field]: value },
    }));
  };

  const updateLlm = (field: keyof LlmConfig, value: string) => {
    setConfig((prev) => ({
      ...prev,
      llm: { ...prev.llm, [field]: value },
    }));
  };

  const formatTime = (iso: string) => {
    const date = new Date(iso);
    return date.toLocaleString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  return (
    <div className="app-container">
      <header className="app-header">
        <h1>NoType Mini</h1>
        <p>
          {config.recording_mode === "push_to_talk" ? (
            <>
              按住 <span className="shortcut-hint">⌘+.</span> 说话，松开后自动输入。
            </>
          ) : (
            <>
              按 <span className="shortcut-hint">⌘+.</span> 开始录音，再按结束，Esc 取消。
            </>
          )}
        </p>
      </header>

      <section className="card">
        <h2>状态</h2>
        <div className={`status-text ${statusType}`}>{status}</div>
        {lastResult && (
          <div className="result-box">{lastResult}</div>
        )}
      </section>

      <section className="card">
        <h2>统计</h2>
        <div style={{ display: "flex", gap: 24, fontSize: 14 }}>
          <div>
            <div style={{ color: "#6e6e73" }}>累计字数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.total_words}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>今日字数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.today_words}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>累计次数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.total_sessions}</div>
          </div>
        </div>
      </section>

      <section className="card">
        <h2>DashScope 语音识别引擎</h2>
        <div className="form-group">
          <label>Base URL</label>
          <input
            type="text"
            value={config.dashscope.base_url}
            onChange={(e) => updateDashScope("base_url", e.target.value)}
            placeholder="https://dashscope.aliyuncs.com"
          />
        </div>
        <div className="form-group">
          <label>API Key</label>
          <input
            type="password"
            value={config.dashscope.api_key}
            onChange={(e) => updateDashScope("api_key", e.target.value)}
            placeholder="sk-xxx"
          />
        </div>
        <div className="form-group">
          <label>Model</label>
          <input
            type="text"
            value={config.dashscope.model}
            onChange={(e) => updateDashScope("model", e.target.value)}
            placeholder="paraformer-v2 或 qwen3-asr-flash"
          />
        </div>
        <div className="button-row">
          <button className="btn-primary" onClick={handleSave}>
            保存配置
          </button>
          <button className="btn-secondary" onClick={handleTestDashScope}>
            测试连接
          </button>
        </div>
      </section>

      <section className="card">
        <h2>LLM 润色引擎</h2>
        <div className="form-group">
          <label>Base URL</label>
          <input
            type="text"
            value={config.llm.base_url}
            onChange={(e) => updateLlm("base_url", e.target.value)}
            placeholder="https://api.deepseek.com"
          />
        </div>
        <div className="form-group">
          <label>API Key</label>
          <input
            type="password"
            value={config.llm.api_key}
            onChange={(e) => updateLlm("api_key", e.target.value)}
            placeholder="sk-xxx"
          />
        </div>
        <div className="form-group">
          <label>Model</label>
          <input
            type="text"
            value={config.llm.model}
            onChange={(e) => updateLlm("model", e.target.value)}
            placeholder="deepseek-chat"
          />
        </div>
        <div className="button-row">
          <button className="btn-secondary" onClick={handleTestLlm}>
            测试连接
          </button>
        </div>
      </section>

      <section className="card">
        <h2>输出设置</h2>
        <div className="form-group">
          <label>录音模式</label>
          <select
            value={config.recording_mode}
            onChange={(e) =>
              setConfig((prev) => ({
                ...prev,
                recording_mode: e.target.value as "push_to_talk" | "continuous",
              }))
            }
            style={{
              width: "100%",
              padding: "10px 12px",
              borderRadius: 8,
              border: "1px solid #d2d2d7",
              fontSize: 14,
            }}
          >
            <option value="push_to_talk">按住说话（按住 ⌘+. 录音，松开结束）</option>
            <option value="continuous">连续录音（按 ⌘+. 开始，再按结束，Esc 取消）</option>
          </select>
        </div>
        <div className="form-group">
          <label>输出模式</label>
          <select
            value={config.output_mode}
            onChange={(e) =>
              setConfig((prev) => ({ ...prev, output_mode: e.target.value }))
            }
            style={{
              width: "100%",
              padding: "10px 12px",
              borderRadius: 8,
              border: "1px solid #d2d2d7",
              fontSize: 14,
            }}
          >
            <option value="polish">智能整理（去口头禅、自动排版）</option>
            <option value="verbatim">逐字转写（保留原样）</option>
          </select>
        </div>
        <div className="form-group">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={config.use_clipboard_fallback}
              onChange={(e) =>
                setConfig((prev) => ({
                  ...prev,
                  use_clipboard_fallback: e.target.checked,
                }))
              }
            />
            长文本或输入失败时改用剪贴板粘贴（Cmd+V）
          </label>
        </div>
        <div className="form-group">
          <label>剪贴板回退字数阈值</label>
          <input
            type="number"
            min={0}
            value={config.clipboard_fallback_threshold}
            onChange={(e) =>
              setConfig((prev) => ({
                ...prev,
                clipboard_fallback_threshold: Number(e.target.value),
              }))
            }
            placeholder="100"
          />
        </div>
        <div className="form-group">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={config.auto_enter}
              onChange={(e) =>
                setConfig((prev) => ({ ...prev, auto_enter: e.target.checked }))
              }
            />
            输入完成后自动按回车（适合聊天软件）
          </label>
        </div>
      </section>

      <section className="card">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: 16,
          }}
        >
          <h2 style={{ margin: 0 }}>历史记录</h2>
          <div className="button-row" style={{ margin: 0 }}>
            <button className="btn-secondary" onClick={handleExport}>
              导出 Markdown
            </button>
            <button className="btn-secondary" onClick={handleClearHistory}>
              清空
            </button>
          </div>
        </div>

        {history.length === 0 ? (
          <div style={{ color: "#6e6e73", fontSize: 14, padding: "20px 0" }}>
            暂无历史记录
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {history.map((entry) => (
              <div
                key={entry.id}
                style={{
                  background: "#f5f5f7",
                  borderRadius: 8,
                  padding: 12,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                    marginBottom: 8,
                    fontSize: 12,
                    color: "#6e6e73",
                  }}
                >
                  <span>{formatTime(entry.timestamp)}</span>
                  <span>{entry.word_count} 字</span>
                </div>
                <div
                  style={{
                    fontSize: 14,
                    lineHeight: 1.6,
                    marginBottom: 8,
                    whiteSpace: "pre-wrap",
                  }}
                >
                  {entry.polished_text}
                </div>
                <div className="button-row" style={{ margin: 0 }}>
                  <button
                    className="btn-secondary"
                    style={{ padding: "6px 12px", fontSize: 12 }}
                    onClick={() => handleCopy(entry.polished_text)}
                  >
                    复制
                  </button>
                  <button
                    className="btn-secondary"
                    style={{ padding: "6px 12px", fontSize: 12 }}
                    onClick={() => handleDelete(entry.id)}
                  >
                    删除
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="card">
        <h2>使用说明</h2>
        <ul style={{ fontSize: 14, color: "#3a3a3c", paddingLeft: 18 }}>
          <li>
            <strong>按住说话模式：</strong>按住 <strong>⌘+.</strong> 开始录音，屏幕中央会出现音量气泡；松开自动识别并输入。
          </li>
          <li>
            <strong>连续录音模式：</strong>按一次 <strong>⌘+.</strong> 开始录音，再按一次结束；录音中按 <strong>Esc</strong> 取消。
          </li>
          <li>
            程序会自动识别、整理，并把文字输入到当前光标处。长文本或输入失败时会改用剪贴板粘贴。
          </li>
          <li>第一次使用需要在系统设置中授权麦克风 + 辅助功能权限。</li>
          <li>关闭主窗口后应用会常驻在系统托盘，右键托盘图标可显示/隐藏窗口或退出。</li>
        </ul>
      </section>
    </div>
  );
}

export default App;
