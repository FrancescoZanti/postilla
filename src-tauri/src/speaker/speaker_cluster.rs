use super::speaker_engine::SpeakerClusterEngine;

pub struct AgglomerativeCluster {
    distance_threshold: f64,
}

impl AgglomerativeCluster {
    pub fn new() -> Self {
        Self {
            distance_threshold: 0.35,
        }
    }

    pub fn with_threshold(threshold: f64) -> Self {
        Self {
            distance_threshold: threshold,
        }
    }
}

fn cosine_distance(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a < 1e-10 || norm_b < 1e-10 {
        return 1.0;
    }
    1.0 - (dot / (norm_a * norm_b))
}

impl SpeakerClusterEngine for AgglomerativeCluster {
    fn cluster(&mut self, embeddings: &[Vec<f64>]) -> Result<Vec<usize>, String> {
        let n = embeddings.len();
        if n == 0 {
            return Ok(vec![]);
        }
        if n == 1 {
            return Ok(vec![0]);
        }

        // Start with each point in its own cluster
        let mut clusters: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
        let max_clusters = n.min(6);

        loop {
            if clusters.len() <= 1 || clusters.len() <= max_clusters {
                break;
            }

            let mut min_dist = f64::MAX;
            let mut merge_pair = (0, 0);

            for i in 0..clusters.len() {
                for j in (i + 1)..clusters.len() {
                    let dist = cluster_distance(&clusters[i], &clusters[j], embeddings);
                    if dist < min_dist {
                        min_dist = dist;
                        merge_pair = (i, j);
                    }
                }
            }

            if min_dist > self.distance_threshold && clusters.len() <= max_clusters {
                break;
            }

            if min_dist > 1.0 {
                break;
            }

            // Merge clusters
            let j = merge_pair.1;
            let i = merge_pair.0;
            let mut merged = clusters.remove(j);
            clusters[i].append(&mut merged);
        }

        // Assign cluster IDs
        let mut labels = vec![0usize; n];
        for (cluster_id, indices) in clusters.iter().enumerate() {
            for &idx in indices {
                labels[idx] = cluster_id;
            }
        }

        Ok(labels)
    }
}

fn cluster_distance(a: &[usize], b: &[usize], embeddings: &[Vec<f64>]) -> f64 {
    let mut min_dist = f64::MAX;
    for &i in a {
        for &j in b {
            let d = cosine_distance(&embeddings[i], &embeddings[j]);
            if d < min_dist {
                min_dist = d;
            }
        }
    }
    min_dist
}
