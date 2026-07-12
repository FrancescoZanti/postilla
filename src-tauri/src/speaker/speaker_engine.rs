use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSegment {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerSegment {
    pub start: f64,
    pub end: f64,
    pub speaker_id: usize,
    pub speaker_label: String,
}

pub trait VoiceActivityDetector {
    fn detect(&mut self, audio_path: &str) -> Result<Vec<AudioSegment>, String>;
}

pub trait SpeakerEmbeddingEngine {
    fn compute_embedding(&mut self, audio_segment_path: &str) -> Result<Vec<f64>, String>;
}

pub trait SpeakerClusterEngine {
    fn cluster(&mut self, embeddings: &[Vec<f64>]) -> Result<Vec<usize>, String>;
}

pub trait SpeakerMatcher {
    fn match_embedding(&self, embedding: &[f64], known_speakers: &[(i64, String, Vec<f64>)]) -> Option<(i64, String, f64)>;
}
