use std::process::Command;
use super::speaker_engine::SpeakerEmbeddingEngine;

pub struct SpectralEmbeddingEngine;

impl SpectralEmbeddingEngine {
    pub fn new() -> Self {
        Self
    }

    fn compute_features(samples: &[f32], sample_rate: u32) -> Vec<f64> {
        if samples.is_empty() {
            return vec![0.0; 6];
        }

        let frame_size = (sample_rate as f32 * 0.032) as usize; // 32ms frames
        let frame_size = frame_size.max(256).min(4096);

        let num_frames = samples.len() / frame_size;
        if num_frames == 0 {
            return vec![0.0; 6];
        }

        let mut frame_rms = Vec::with_capacity(num_frames);
        let mut frame_zcr = Vec::with_capacity(num_frames);
        let mut frame_hf_ratio = Vec::with_capacity(num_frames);

        for i in 0..num_frames {
            let start = i * frame_size;
            let end = start + frame_size.min(samples.len() - start);
            let frame = &samples[start..end];

            // RMS
            let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
            let rms = (sum_sq / frame.len() as f32).sqrt();
            frame_rms.push(rms as f64);

            // Zero-crossing rate
            let mut crossings = 0;
            for w in frame.windows(2) {
                if w[0].signum() != w[1].signum() && w[0] != 0.0 {
                    crossings += 1;
                }
            }
            frame_zcr.push(crossings as f64 / frame.len() as f64);

            // High-frequency ratio (energy above ~2kHz relative to total)
            let mut hp_energy = 0.0f32;
            let mut total_energy = 0.0f32;
            for w in frame.windows(2) {
                let diff = w[1] - w[0];
                hp_energy += diff * diff;
                total_energy += w[1] * w[1];
            }
            let ratio = if total_energy > 1e-10 {
                (hp_energy / total_energy).min(1.0)
            } else {
                0.0
            };
            frame_hf_ratio.push(ratio as f64);
        }

        // Aggregate features
        let mean_rms = mean(&frame_rms);
        let var_rms = variance(&frame_rms, mean_rms);
        let mean_zcr = mean(&frame_zcr);
        let var_zcr = variance(&frame_zcr, mean_zcr);
        let mean_hf = mean(&frame_hf_ratio);
        let dur_secs = samples.len() as f64 / sample_rate as f64;

        vec![
            mean_rms,
            var_rms.sqrt(), // std dev of RMS
            mean_zcr,
            var_zcr.sqrt(),
            mean_hf,
            dur_secs.min(30.0) / 30.0, // normalized duration, cap at 30s
        ]
    }
}

impl SpeakerEmbeddingEngine for SpectralEmbeddingEngine {
    fn compute_embedding(&mut self, audio_segment_path: &str) -> Result<Vec<f64>, String> {
        let output = Command::new("ffmpeg")
            .arg("-i")
            .arg(audio_segment_path)
            .arg("-f")
            .arg("f32le")
            .arg("-acodec")
            .arg("pcm_f32le")
            .arg("-ac")
            .arg("1")
            .arg("-ar")
            .arg("16000")
            .arg("-")
            .output()
            .map_err(|e| format!("Failed to decode audio segment: {}", e))?;

        if output.stdout.is_empty() {
            return Err("Empty audio segment".to_string());
        }

        let bytes = &output.stdout;
        let samples: &[f32] = unsafe {
            std::slice::from_raw_parts(
                bytes.as_ptr() as *const f32,
                bytes.len() / 4,
            )
        };

        Ok(Self::compute_features(samples, 16000))
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn variance(values: &[f64], mean_val: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    values.iter().map(|v| (v - mean_val).powi(2)).sum::<f64>() / values.len() as f64
}
