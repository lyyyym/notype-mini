import { useState, useEffect, useRef } from "react";
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

interface DictionaryEntry {
  from: string;
  to: string;
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
  edit_shortcut: string;
  dictionary: DictionaryEntry[];
  config_version: number;
}

interface HistoryEntry {
  id: string;
  timestamp: string;
  raw_text: string;
  polished_text: string;
  word_count: number;
  duration_ms: number;
  entry_type: "transcribe" | "edit";
}

interface HistoryStats {
  transcribe_total_words: number;
  transcribe_today_words: number;
  transcribe_total_sessions: number;
  edit_total_words: number;
  edit_today_words: number;
  edit_total_sessions: number;
}

interface RecordingStateEvent {
  state: string;
  mode?: "transcribe" | "edit";
}

interface TranscriptionEvent {
  text: string;
  entry_type: "transcribe" | "edit";
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
    edit_shortcut: "Command+Option+Period",
    dictionary: [],
    config_version: 1,
  });

  // 用 ref 保存最新配置，供声音反馈等回调读取
  const configRef = useRef(config);
  configRef.current = config;

  const [status, setStatus] = useState("等待开始...");
  const [statusType, setStatusType] = useState<"" | "success" | "error">("");
  const [lastResult, setLastResult] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [stats, setStats] = useState<HistoryStats>({
    transcribe_total_words: 0,
    transcribe_today_words: 0,
    transcribe_total_sessions: 0,
    edit_total_words: 0,
    edit_today_words: 0,
    edit_total_sessions: 0,
  });

  // 词典新增输入
  const [newFrom, setNewFrom] = useState("");
  const [newTo, setNewTo] = useState("");
  // 词典是否有未保存的更改（添加/删除后、保存成功前为 true）
  const [dictDirty, setDictDirty] = useState(false);

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

  // 声音反馈
  const playTone = (
    freq: number,
    dur = 0.09,
    gain = 0.045,
    type: OscillatorType = "sine",
    delay = 0
  ) => {
    if (!configRef.current.sound_feedback) return;
    try {
      const audioCtx = new (window.AudioContext || (window as any).webkitAudioContext)();
      if (audioCtx.state === "suspended") {
        void audioCtx.resume();
        if (audioCtx.state === "suspended") return;
      }
      const t = audioCtx.currentTime + delay;
      const osc = audioCtx.createOscillator();
      const g = audioCtx.createGain();
      osc.type = type;
      osc.frequency.value = freq;
      g.gain.setValueAtTime(gain, t);
      g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
      osc.connect(g).connect(audioCtx.destination);
      osc.start(t);
      osc.stop(t + dur);
    } catch {
      // best effort
    }
  };

  const sounds = {
    start: () => playTone(659, 0.07),
    done: () => {
      playTone(784, 0.07);
      playTone(1175, 0.11, 0.04, "sine", 0.07);
    },
    error: () => playTone(196, 0.16, 0.04, "square"),
  };

  useEffect(() => {
    // 加载配置
    invoke<Config>("get_config").then((loaded) => {
      setConfig(loaded);
    });

    loadHistory();
    loadStats();

    // 监听事件
    let prevState = "idle";
    const unlistenState = listen<RecordingStateEvent>("recording-state", (event) => {
      const { state, mode } = event.payload;
      if (state === "recording" && prevState !== "recording") {
        if (mode === "edit") {
          setStatus("正在听取编辑指令...");
        } else {
          setStatus("正在录音...");
        }
        setStatusType("");
        sounds.start();
      } else if (state === "processing") {
        setStatus("正在识别/整理...");
        setStatusType("");
      } else if (state === "idle") {
        setStatus("等待开始...");
      }
      prevState = state;
    });

    const unlistenResult = listen<TranscriptionEvent>("transcription-result", (event) => {
      setLastResult(event.payload.text);
      if (event.payload.entry_type === "edit") {
        setStatus("编辑完成并已输入");
      } else {
        setStatus("识别完成并已输入");
      }
      setStatusType("success");
      sounds.done();
      loadHistory();
      loadStats();
    });

    const unlistenError = listen<{ code: string; message: string }>("error", (event) => {
      setStatus(`错误: ${event.payload.message}`);
      setStatusType("error");
      sounds.error();
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
      setDictDirty(false);
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

  // 词典操作
  const handleAddDictionaryEntry = () => {
    const from = newFrom.trim();
    const to = newTo.trim();

    if (!from) {
      setStatus("错误：听错词不能为空");
      setStatusType("error");
      return;
    }

    const exists = config.dictionary.some(
      (entry) => entry.from.trim().toLowerCase() === from.toLowerCase()
    );
    if (exists) {
      setStatus("错误：已存在相同的听错词");
      setStatusType("error");
      return;
    }

    setConfig((prev) => ({
      ...prev,
      dictionary: [...prev.dictionary, { from, to }],
    }));
    setDictDirty(true);
    setStatus(`已添加「${from} → ${to}」，保存配置后才会生效`);
    setStatusType("success");
    setNewFrom("");
    setNewTo("");
  };

  const handleDeleteDictionaryEntry = (index: number) => {
    setConfig((prev) => ({
      ...prev,
      dictionary: prev.dictionary.filter((_, i) => i !== index),
    }));
    setDictDirty(true);
    setStatus("词条已删除，保存配置后才会生效");
    setStatusType("success");
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

  const entryTypeLabel = (type: string) => {
    return type === "edit" ? "编辑" : "录音";
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
        <div style={{ display: "flex", gap: 24, fontSize: 14, flexWrap: "wrap" }}>
          <div>
            <div style={{ color: "#6e6e73" }}>录音累计字数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.transcribe_total_words}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>录音今日字数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.transcribe_today_words}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>录音累计次数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.transcribe_total_sessions}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>编辑累计字数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.edit_total_words}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>编辑今日字数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.edit_today_words}</div>
          </div>
          <div>
            <div style={{ color: "#6e6e73" }}>编辑累计次数</div>
            <div style={{ fontSize: 24, fontWeight: 600 }}>{stats.edit_total_sessions}</div>
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
          <label>编辑快捷键</label>
          <input
            type="text"
            value={config.edit_shortcut}
            onChange={(e) =>
              setConfig((prev) => ({ ...prev, edit_shortcut: e.target.value }))
            }
            placeholder="Command+Option+Period"
          />
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
        <h2>个人词典</h2>
        <p style={{ fontSize: 13, color: "#6e6e73", marginTop: -8 }}>
          把识别错误的词映射到正确写法，会在识别后、润色前自动替换。
        </p>

        {dictDirty && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
              fontSize: 13,
              color: "#b25000",
              background: "#fff4e5",
              border: "1px solid #ffd9a8",
              padding: "8px 12px",
              borderRadius: 8,
              marginBottom: 12,
            }}
          >
            <span>词典有未保存的更改，保存后才会生效</span>
            <button
              className="btn-primary"
              style={{ padding: "4px 12px", fontSize: 12 }}
              onClick={handleSave}
            >
              立即保存
            </button>
          </div>
        )}

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 8,
            marginBottom: 16,
          }}
        >
          {config.dictionary.map((entry, index) => (
            <div
              key={index}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                background: "#f5f5f7",
                padding: "8px 12px",
                borderRadius: 8,
              }}
            >
              <span style={{ flex: 1, fontSize: 14 }}>{entry.from}</span>
              <span style={{ color: "#6e6e73" }}>→</span>
              <span style={{ flex: 1, fontSize: 14 }}>{entry.to}</span>
              <button
                className="btn-secondary"
                style={{ padding: "4px 10px", fontSize: 12 }}
                onClick={() => handleDeleteDictionaryEntry(index)}
              >
                删除
              </button>
            </div>
          ))}
        </div>

        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            flexWrap: "wrap",
          }}
        >
          <input
            type="text"
            placeholder="听错词"
            value={newFrom}
            onChange={(e) => setNewFrom(e.target.value)}
            style={{ flex: 1, minWidth: 120 }}
          />
          <span style={{ color: "#6e6e73" }}>→</span>
          <input
            type="text"
            placeholder="正确词"
            value={newTo}
            onChange={(e) => setNewTo(e.target.value)}
            style={{ flex: 1, minWidth: 120 }}
          />
          <button className="btn-secondary" onClick={handleAddDictionaryEntry}>
            添加
          </button>
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
          <div className="history-list">
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
                  <span>
                    [{entryTypeLabel(entry.entry_type)}] {formatTime(entry.timestamp)}
                  </span>
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
            <strong>语音编辑：</strong>在任意应用中先选中文本，再按住 <strong>⌘+Option+.</strong>
            说出指令（如“改正式”“翻译成英文”），松开自动替换选中文本；没有选中文本时会把指令当作自由生成。
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
