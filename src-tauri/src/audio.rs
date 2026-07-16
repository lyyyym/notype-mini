use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use tauri::{AppHandle, Emitter};

pub struct RecorderHandle {
    stop_tx: mpsc::Sender<()>,
    result_rx: mpsc::Receiver<Result<(Vec<i16>, u32), String>>,
}

impl RecorderHandle {
    pub fn stop(self) -> Result<(Vec<i16>, u32), String> {
        let _ = self.stop_tx.send(());
        self.result_rx
            .recv()
            .unwrap_or_else(|_| Err("录音线程异常退出".to_string()))
    }
}

fn emit_volume(app: &AppHandle, level: f32) {
    #[derive(Clone, serde::Serialize)]
    struct VolumeEvent {
        level: f32,
    }
    let _ = app.emit("volume-level", VolumeEvent { level });
}

pub fn start_recording(app_handle: AppHandle) -> Result<RecorderHandle, anyhow::Error> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel();
    let (volume_tx, volume_rx) = mpsc::channel::<f32>();

    let volume_app = app_handle.clone();

    // 音量发射线程：把音量事件转发到前端
    std::thread::spawn(move || {
        while let Ok(level) = volume_rx.recv() {
            emit_volume(&volume_app, level);
        }
    });

    // 录音线程：在独立线程中创建并持有 cpal Stream（Stream 不能跨线程移动）
    std::thread::spawn(move || {
        let result = recording_thread(stop_rx, volume_tx, app_handle);
        let _ = result_tx.send(result);
    });

    Ok(RecorderHandle {
        stop_tx,
        result_rx,
    })
}

fn recording_thread(
    stop_rx: mpsc::Receiver<()>,
    volume_tx: mpsc::Sender<f32>,
    _app_handle: AppHandle,
) -> Result<(Vec<i16>, u32), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "未找到麦克风设备".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|e| e.to_string())?;
    let sample_rate = config.sample_rate().0;

    let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let samples_clone = samples.clone();

    let err_fn = move |err| eprintln!("录音流错误: {}", err);

    let stream = match config.sample_format() {
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let mut samples = samples_clone.lock().unwrap();
                samples.extend_from_slice(data);

                if !data.is_empty() {
                    let sum: f64 = data.iter().map(|&s| (s as f64).powi(2)).sum();
                    let rms = (sum / data.len() as f64).sqrt();
                    let normalized = (rms / i16::MAX as f64).min(1.0) as f32;
                    let _ = volume_tx.send(normalized);
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut samples = samples_clone.lock().unwrap();
                for &sample in data {
                    let s = (sample * i16::MAX as f32) as i16;
                    samples.push(s);
                }

                if !data.is_empty() {
                    let sum: f64 = data.iter().map(|&s| (s as f64).powi(2)).sum();
                    let rms = (sum / data.len() as f64).sqrt();
                    let normalized = rms.min(1.0) as f32;
                    let _ = volume_tx.send(normalized);
                }
            },
            err_fn,
            None,
        ),
        format => {
            return Err(format!("不支持的音频采样格式: {:?}", format));
        }
    }
    .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    stop_rx
        .recv()
        .map_err(|_| "停止信号接收失败".to_string())?;
    let _ = stream.pause();
    drop(stream); // 释放 Stream，回调中的 volume_tx 随之释放，音量线程退出

    let samples = match std::sync::Arc::try_unwrap(samples) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().map(|g| g.clone()).unwrap_or_default(),
    };

    Ok((samples, sample_rate))
}

pub fn samples_to_wav(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>, anyhow::Error> {
    let mut buffer = Vec::new();
    {
        let mut writer = hound::WavWriter::new(
            std::io::Cursor::new(&mut buffer),
            hound::WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        for &sample in samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
    }
    Ok(buffer)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_samples_to_wav_produces_valid_wav() {
        let samples: Vec<i16> = (0..1000).map(|i| (i as i16).wrapping_mul(10)).collect();
        let wav = samples_to_wav(&samples, 16000).expect("编码 WAV 失败");

        assert!(wav.len() > 44, "WAV 数据太短");
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn test_samples_to_wav_empty_samples() {
        let wav = samples_to_wav(&[], 16000).expect("空样本编码失败");
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }
}
