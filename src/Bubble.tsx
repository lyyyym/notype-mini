import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

function Bubble() {
  const [state, setState] = useState<"idle" | "recording" | "processing">("idle");
  const [volume, setVolume] = useState(0);
  const [timer, setTimer] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    const unlistenState = listen<{ state: string }>("recording-state", (event) => {
      const newState = event.payload.state as "idle" | "recording" | "processing";
      setState(newState);

      if (newState === "recording") {
        setTimer(0);
        timerRef.current = setInterval(() => {
          setTimer((t) => t + 1);
        }, 1000);
      } else {
        if (timerRef.current) {
          clearInterval(timerRef.current);
          timerRef.current = null;
        }
      }
    });

    const unlistenVolume = listen<{ level: number }>("volume-level", (event) => {
      setVolume(event.payload.level);
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenVolume.then((fn) => fn());
      if (timerRef.current) {
        clearInterval(timerRef.current);
      }
    };
  }, []);

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  };

  const ringScale = state === "recording" ? 1 + volume * 0.3 : 1;

  return (
    <div className="bubble-container">
      {state === "processing" ? (
        <div className="bubble-processing">
          <div className="bubble-spinner" />
          <div className="bubble-status">识别中...</div>
        </div>
      ) : (
        <div
          className={`bubble-ring ${state === "recording" ? "active" : ""}`}
          style={{ transform: `scale(${ringScale})` }}
        >
          <div className="bubble-inner">
            <div className="bubble-icon">{state === "recording" ? "🎙️" : "🎤"}</div>
            <div className="bubble-status">
              {state === "recording" ? "正在录音" : "准备就绪"}
            </div>
            {state === "recording" && (
              <div className="bubble-timer">{formatTime(timer)}</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default Bubble;
