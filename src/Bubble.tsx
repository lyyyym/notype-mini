import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";

const LEVEL_WEIGHTS = [0.45, 0.62, 0.85, 1, 0.9, 1, 0.85, 0.62, 0.45];

type BubbleState = "idle" | "recording" | "processing" | "result" | "error";

function Bubble() {
  const [state, setState] = useState<BubbleState>("idle");
  const [mode, setMode] = useState<"transcribe" | "edit" | undefined>(undefined);
  const [text, setText] = useState("");
  const [timer, setTimer] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const barsRef = useRef<HTMLElement[]>([]);
  const smoothedLevelRef = useRef(0);
  const rafRef = useRef<number | null>(null);

  const resetWave = useCallback(() => {
    smoothedLevelRef.current = 0;
    barsRef.current.forEach((bar) => {
      if (bar) bar.style.height = "";
    });
  }, []);

  const setLevel = useCallback((level: number) => {
    if (level <= 0) {
      resetWave();
      return;
    }
    const amp = Math.min(1, level * 7);
    smoothedLevelRef.current = smoothedLevelRef.current * 0.55 + amp * 0.45;
    barsRef.current.forEach((bar, i) => {
      if (!bar) return;
      const jitter = 0.82 + Math.random() * 0.36;
      const h = 4 + LEVEL_WEIGHTS[i] * smoothedLevelRef.current * 19 * jitter;
      bar.style.height = `${Math.max(3, Math.min(22, h))}px`;
    });
  }, [resetWave]);

  // 动画循环：让波形在即使没有新 volume 事件时也保持动态
  useEffect(() => {
    const tick = () => {
      if (state === "recording" && smoothedLevelRef.current > 0.01) {
        barsRef.current.forEach((bar, i) => {
          if (!bar) return;
          const jitter = 0.82 + Math.random() * 0.36;
          const h = 4 + LEVEL_WEIGHTS[i] * smoothedLevelRef.current * 19 * jitter;
          bar.style.height = `${Math.max(3, Math.min(22, h))}px`;
        });
      }
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [state]);

  useEffect(() => {
    const unlistenState = listen<{ state: string; mode?: "transcribe" | "edit" }>(
      "recording-state",
      (event) => {
        const newState = event.payload.state;
        setMode(event.payload.mode);

        if (newState === "recording") {
          setState("recording");
          setText("");
          setTimer(0);
          resetWave();
          timerRef.current = setInterval(() => {
            setTimer((t) => t + 1);
          }, 1000);
        } else if (newState === "processing") {
          setState("processing");
          if (timerRef.current) {
            clearInterval(timerRef.current);
            timerRef.current = null;
          }
        } else if (newState === "idle") {
          setState("idle");
          setText("");
          if (timerRef.current) {
            clearInterval(timerRef.current);
            timerRef.current = null;
          }
        }
      }
    );

    const unlistenVolume = listen<{ level: number }>("volume-level", (event) => {
      setLevel(event.payload.level);
    });

    const unlistenResult = listen<{ text: string; entry_type: string }>(
      "transcription-result",
      (event) => {
        setText(event.payload.text);
        setState("result");
      }
    );

    const unlistenError = listen<{ code: string; message: string }>("error", (event) => {
      setText(event.payload.message);
      setState("error");
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenVolume.then((fn) => fn());
      unlistenResult.then((fn) => fn());
      unlistenError.then((fn) => fn());
      if (timerRef.current) clearInterval(timerRef.current);
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [setLevel, resetWave]);

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  };

  const statusText =
    state === "recording"
      ? mode === "edit"
        ? "正在听取编辑指令"
        : "正在录音"
      : state === "processing"
      ? "识别中..."
      : state === "result"
      ? ""
      : state === "error"
      ? ""
      : "准备就绪";

  return (
    <div className="bubble-container">
      <div className={`card ${text ? "text-mode" : ""}`} id="card">
        <div className="card-content" id="card-content">
          {state === "recording" && !text && (
            <div className="typing-dots" id="dots">
              <i></i>
              <i></i>
              <i></i>
            </div>
          )}
          {text && <div className="result-text">{text}</div>}
        </div>
      </div>

      <div className={`pill ${state}`} id="pill">
        {state === "recording" && (
          <span className="waveform">
            {LEVEL_WEIGHTS.map((_, i) => (
              <i
                key={i}
                ref={(el) => {
                  if (el) barsRef.current[i] = el;
                }}
              />
            ))}
          </span>
        )}
        {state === "processing" && <span className="pill-text">{statusText}</span>}
        {(state === "result" || state === "error") && (
          <span className="pill-text">{state === "result" ? "完成" : "错误"}</span>
        )}
        {state !== "recording" && state !== "processing" && state !== "result" && state !== "error" && (
          <span className="pill-text">{statusText}</span>
        )}
      </div>

      {state === "recording" && (
        <div className="timer">{formatTime(timer)}</div>
      )}
    </div>
  );
}

export default Bubble;
