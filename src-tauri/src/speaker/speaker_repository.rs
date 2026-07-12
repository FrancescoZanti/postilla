use rusqlite::Connection;
use crate::db::{self, Speaker, TranscriptBlock};

pub struct SpeakerRepo;

impl SpeakerRepo {
    pub fn get_blocks_for_session(conn: &Connection, session_id: i64) -> Vec<TranscriptBlock> {
        db::get_transcript_blocks(conn, session_id)
    }

    pub fn delete_blocks_for_session(conn: &Connection, session_id: i64) {
        db::delete_transcript_blocks(conn, session_id)
    }

    pub fn insert_block(conn: &Connection, block: &TranscriptBlock) -> Result<i64, String> {
        db::insert_transcript_block(conn, block).map_err(|e| e.to_string())
    }

    pub fn get_all_speakers(conn: &Connection) -> Vec<Speaker> {
        db::get_all_speakers(conn)
    }

    pub fn get_speaker_by_id(conn: &Connection, id: i64) -> Option<Speaker> {
        db::get_speaker_by_id(conn, id)
    }

    pub fn upsert_speaker(conn: &Connection, speaker: &Speaker) -> Result<i64, String> {
        db::upsert_speaker(conn, speaker).map_err(|e| e.to_string())
    }

    pub fn rename_speaker(conn: &Connection, id: i64, new_name: &str) -> Result<(), String> {
        db::rename_speaker(conn, id, new_name).map_err(|e| e.to_string())
    }

    pub fn delete_speaker(conn: &Connection, id: i64) -> Result<(), String> {
        db::delete_speaker(conn, id).map_err(|e| e.to_string())
    }

    pub fn get_known_speaker_embeddings(conn: &Connection) -> Vec<(i64, String, Vec<f64>)> {
        db::get_all_speakers(conn)
            .into_iter()
            .filter_map(|s| {
                s.embedding.map(|emb| (s.id, s.display_name, emb))
            })
            .collect()
    }
}
