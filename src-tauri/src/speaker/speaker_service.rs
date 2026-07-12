use std::path::Path;
use std::process::Command;

use rusqlite::Connection;

use crate::db::TranscriptBlock;
use crate::speaker::speaker_vad::FfmpegVAD;
use crate::speaker::speaker_embedding::SpectralEmbeddingEngine;
use crate::speaker::speaker_cluster::AgglomerativeCluster;
use crate::speaker::speaker_matching::CosineMatcher;
use crate::speaker::speaker_repository::SpeakerRepo;

use super::speaker_engine::{
    SpeakerEmbeddingEngine, SpeakerClusterEngine,
    VoiceActivityDetector, SpeakerMatcher,
};

pub struct SpeakerService;

impl SpeakerService {
    /// Run the full diarization pipeline:
    /// 1. VAD → speech segments with timestamps
    /// 2. Per-segment embedding → cluster → speaker labels
    /// 3. Match against known speakers
    /// 4. Build structured transcript_blocks
    /// 5. Update sessions.transcript with formatted text
    pub fn run_diarization_pipeline(
        conn: &Connection,
        app_data_dir: &Path,
        session_id: i64,
        audio_path: &str,
        full_transcript: &str,
    ) -> Result<Vec<TranscriptBlock>, String> {
        // 1. Voice Activity Detection
        let mut vad = FfmpegVAD::new();
        let segments = vad.detect(audio_path)?;

        if segments.is_empty() {
            return Err("No speech segments detected".to_string());
        }

        // 2. Extract each segment and compute embedding
        let media_dir = app_data_dir.join("media").join("segments");
        std::fs::create_dir_all(&media_dir)
            .map_err(|e| format!("Failed to create segments dir: {}", e))?;

        let mut embeddings: Vec<Vec<f64>> = Vec::new();
        let mut seg_times: Vec<(f64, f64)> = Vec::new();
        let mut embedding_engine = SpectralEmbeddingEngine::new();

        // Process up to 50 segments max for performance
        let max_segments = segments.len().min(50);
        let segments: Vec<_> = segments.iter().take(max_segments).collect();

        for (idx, seg) in segments.iter().enumerate() {
            let seg_path = media_dir.join(format!("seg_{}_{}.wav", session_id, idx));
            let seg_str = seg_path.to_string_lossy().to_string();

            // Extract segment via ffmpeg (short samples only → fast)
            let extract_output = Command::new("ffmpeg")
                .arg("-y")
                .arg("-i")
                .arg(audio_path)
                .arg("-ss")
                .arg(seg.start.to_string())
                .arg("-to")
                .arg(seg.end.to_string())
                .arg("-ar")
                .arg("16000")
                .arg("-ac")
                .arg("1")
                .arg(&seg_str)
                .output()
                .map_err(|e| format!("Failed to extract segment {}: {}", idx, e))?;

            if !extract_output.status.success() {
                continue;
            }

            // Compute embedding from the segment
            match embedding_engine.compute_embedding(&seg_str) {
                Ok(emb) => embeddings.push(emb),
                Err(_) => continue,
            }

            seg_times.push((seg.start, seg.end));

            // Clean up segment file
            let _ = std::fs::remove_file(&seg_str);
        }

        if embeddings.is_empty() {
            return Err("No segments could be processed for embedding".to_string());
        }

        // 3. Cluster speakers
        let mut cluster = AgglomerativeCluster::new();
        let labels = cluster.cluster(&embeddings)?;

        if labels.len() != seg_times.len() {
            return Err("Cluster output length mismatch".to_string());
        }

        // 4. Match against known speakers from directory
        let known_speakers = SpeakerRepo::get_known_speaker_embeddings(conn);
        let matcher = CosineMatcher::new();

        let num_speakers = labels.iter().max().unwrap_or(&0) + 1;

        // Compute centroid embedding per cluster
        let mut cluster_embeddings: Vec<Vec<f64>> = vec![Vec::new(); num_speakers];
        let mut cluster_counts: Vec<usize> = vec![0; num_speakers];

        for (i, &label) in labels.iter().enumerate() {
            if i < embeddings.len() {
                if cluster_embeddings[label].is_empty() {
                    cluster_embeddings[label] = embeddings[i].clone();
                } else {
                    for j in 0..cluster_embeddings[label].len() {
                        cluster_embeddings[label][j] += embeddings[i][j];
                    }
                }
                cluster_counts[label] += 1;
            }
        }

        for label in 0..num_speakers {
            if cluster_counts[label] > 0 {
                let count = cluster_counts[label] as f64;
                for val in cluster_embeddings[label].iter_mut() {
                    *val /= count;
                }
            }
        }

        // Assign names: try known speakers, fall back to "Persona X"
        let mut speaker_assignments: Vec<(String, Option<i64>)> = Vec::new();
        let mut matched_ids = Vec::new();
        for label in 0..num_speakers {
            let emb = &cluster_embeddings[label];
            match matcher.match_embedding(emb, &known_speakers) {
                Some((id, name, _)) => {
                    speaker_assignments.push((name, Some(id)));
                    matched_ids.push(id);
                }
                None => {
                    let default_name = format!("Persona {}", (b'A' + label as u8) as char);
                    speaker_assignments.push((default_name, None));
                }
            }
        }

        // 5. Build transcript blocks — allocate text from full_transcript
        let blocks = Self::build_blocks(
            session_id,
            &labels,
            &seg_times,
            &speaker_assignments,
            full_transcript,
        );

        // 6. Persist new speakers to the directory
        for label in 0..num_speakers {
            if speaker_assignments[label].1.is_none() {
                let name = &speaker_assignments[label].0;
                let emb = cluster_embeddings[label].clone();
                let now = chrono::Utc::now().to_rfc3339();
                let speaker = crate::db::Speaker {
                    id: 0,
                    display_name: name.clone(),
                    embedding: Some(emb),
                    created_at: now.clone(),
                    updated_at: now,
                };
                if let Ok(_new_id) = SpeakerRepo::upsert_speaker(conn, &speaker) {}
            }
        }

        // 7. Save blocks to DB
        SpeakerRepo::delete_blocks_for_session(conn, session_id);
        let mut saved_blocks = Vec::new();
        for (idx, block) in blocks.iter().enumerate() {
            let mut b = block.clone();
            // Look up speaker_id from assignments
            if let Some(label) = labels.get(idx) {
                if let Some((_, opt_id)) = speaker_assignments.get(*label) {
                    if b.speaker_id.is_none() {
                        b.speaker_id = *opt_id;
                    }
                }
            }
            let new_id = SpeakerRepo::insert_block(conn, &b)?;
            let mut saved = b;
            saved.id = new_id;
            saved_blocks.push(saved);
        }

        // 8. Update sessions.transcript with formatted speaker-labeled text
        let formatted = Self::format_transcript(&saved_blocks);
        conn.execute(
            "UPDATE sessions SET transcript = ?1, status = 'completed' WHERE id = ?2",
            (&formatted, &session_id),
        ).map_err(|e| e.to_string())?;
        crate::db::rebuild_fts(conn);

        Ok(saved_blocks)
    }

    /// Build TranscriptBlocks with text allocated from the full transcript.
    /// Segments are proportionally assigned words from the full transcript.
    fn build_blocks(
        session_id: i64,
        labels: &[usize],
        seg_times: &[(f64, f64)],
        assignments: &[(String, Option<i64>)],
        full_transcript: &str,
    ) -> Vec<TranscriptBlock> {
        let total_dur: f64 = seg_times.iter().map(|(s, e)| e - s).sum();
        if total_dur <= 0.0 {
            return vec![];
        }

        // Split full transcript into words
        let words: Vec<&str> = full_transcript.split_whitespace().collect();
        let total_words = words.len();

        let mut blocks = Vec::new();
        let mut word_idx = 0;

        for (i, (&label, &(start, end))) in labels.iter().zip(seg_times.iter()).enumerate() {
            if i >= assignments.len() {
                break;
            }
            let (ref speaker_label, speaker_id) = assignments[label];
            let dur = end - start;
            let word_count = if total_words > 0 {
                ((dur / total_dur) * total_words as f64).round() as usize
            } else {
                0
            };
            let word_count = word_count.max(1);

            // Get words for this segment
            let end_idx = (word_idx + word_count).min(total_words);
            let seg_words: Vec<&str> = words[word_idx..end_idx].to_vec();
            word_idx = end_idx;

            let text = if seg_words.is_empty() {
                String::new()
            } else {
                let mut t = seg_words.join(" ");
                // Ensure sentence ends with punctuation
                if !t.ends_with('.') && !t.ends_with('!') && !t.ends_with('?') {
                    t.push('.');
                }
                t
            };

            let block = TranscriptBlock {
                id: 0,
                session_id,
                speaker_id,
                speaker_label: Some(speaker_label.clone()),
                start_time: start,
                end_time: end,
                text,
                block_index: i as i64,
            };
            blocks.push(block);
        }

        blocks
    }

    /// Format transcript blocks into the `**Speaker:** text` convention
    pub fn format_transcript(blocks: &[TranscriptBlock]) -> String {
        let mut result = String::new();
        let mut prev_label: Option<String> = None;

        for block in blocks {
            let label = block.speaker_label.as_deref().unwrap_or("Unknown");
            let gap = block.start_time - block.end_time; // gap from PREVIOUS block's end

            // Same speaker, gap < 8s → same paragraph
            if Some(label.to_string()) == prev_label && gap < 8.0 {
                result.push(' ');
                result.push_str(&block.text);
            } else {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&format!("**{}**\n{}\n", label, block.text));
                prev_label = Some(label.to_string());
            }
        }

        result
    }
}
