use std::process::Command;
use super::speaker_engine::{AudioSegment, VoiceActivityDetector};

pub struct FfmpegVAD {
    min_silence_duration: f64,
    silence_threshold: f64,
    min_segment_duration: f64,
}

impl FfmpegVAD {
    pub fn new() -> Self {
        Self {
            min_silence_duration: 0.5,
            silence_threshold: -35.0,
            min_segment_duration: 0.8,
        }
    }
}

impl VoiceActivityDetector for FfmpegVAD {
    fn detect(&mut self, audio_path: &str) -> Result<Vec<AudioSegment>, String> {
        let output = Command::new("ffmpeg")
            .arg("-i")
            .arg(audio_path)
            .arg("-af")
            .arg(format!(
                "silencedetect=noise={}dB:d={}",
                self.silence_threshold,
                self.min_silence_duration
            ))
            .arg("-f")
            .arg("null")
            .arg("-")
            .output()
            .map_err(|e| format!("Failed to run ffmpeg silencedetect: {}", e))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut segments = Vec::new();
        let mut silence_ranges: Vec<(f64, f64)> = Vec::new();

        let mut current_end: Option<f64> = None;
        let mut current_duration: Option<f64> = None;

        for line in stderr.lines() {
            let line = line.trim();
            if let Some(end_str) = line.strip_prefix("[silencedetect") {
                if let Some(end_val) = end_str.split("silence_end:").nth(1) {
                    if let Some(val) = end_val.split_whitespace().next() {
                        if let Ok(end) = val.parse::<f64>() {
                            current_end = Some(end);
                        }
                    }
                }
                if let Some(dur_str) = end_str.split("silence_duration:").nth(1) {
                    if let Some(val) = dur_str.split_whitespace().next() {
                        if let Ok(dur) = val.parse::<f64>() {
                            current_duration = Some(dur);
                        }
                    }
                }
                if let (Some(end), Some(dur)) = (current_end, current_duration) {
                    let start = end - dur;
                    silence_ranges.push((start, end));
                    current_end = None;
                    current_duration = None;
                }
            }
        }

        // Build speech segments from silence gaps
        let mut speech_ranges: Vec<(f64, f64)> = Vec::new();
        let mut prev_end = 0.0;

        for (sil_start, sil_end) in &silence_ranges {
            let speech_end = *sil_start;
            if speech_end - prev_end >= self.min_segment_duration {
                speech_ranges.push((prev_end, speech_end));
            }
            prev_end = *sil_end;
        }

        // Get total duration via ffprobe
        let total_duration = get_audio_duration(audio_path).unwrap_or(prev_end + 10.0);
        if total_duration - prev_end >= self.min_segment_duration {
            speech_ranges.push((prev_end, total_duration));
        }

        for (start, end) in speech_ranges {
            segments.push(AudioSegment { start, end });
        }

        if segments.is_empty() && total_duration > self.min_segment_duration {
            segments.push(AudioSegment {
                start: 0.0,
                end: total_duration,
            });
        }

        Ok(segments)
    }
}

fn get_audio_duration(path: &str) -> Result<f64, String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("csv=p=0")
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe error: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f64>().map_err(|e| format!("Parse error: {}", e))
}
