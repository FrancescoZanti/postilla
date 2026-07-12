use super::speaker_engine::SpeakerMatcher;

pub struct CosineMatcher {
    threshold: f64,
}

impl CosineMatcher {
    pub fn new() -> Self {
        Self { threshold: 0.25 }
    }
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a < 1e-10 || norm_b < 1e-10 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

impl SpeakerMatcher for CosineMatcher {
    fn match_embedding(&self, embedding: &[f64], known_speakers: &[(i64, String, Vec<f64>)]) -> Option<(i64, String, f64)> {
        let mut best: Option<(i64, String, f64)> = None;
        for (id, name, known_emb) in known_speakers {
            let sim = cosine_similarity(embedding, known_emb);
            if sim >= (1.0 - self.threshold) {
                match &best {
                    Some((_, _, best_sim)) if sim > *best_sim => {
                        best = Some((*id, name.clone(), sim));
                    }
                    None => {
                        best = Some((*id, name.clone(), sim));
                    }
                    _ => {}
                }
            }
        }
        best
    }
}
