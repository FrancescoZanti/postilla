# Postilla - Agent Instructions

This project is a local-first desktop application built with **Tauri**.

## Architecture & Tech Stack
- **Frontend:** Web technologies (TypeScript/React - expected in `/src/`)
- **Backend:** Rust (Tauri Core - expected in `/src-tauri/`)
- **Database:** Local SQLite (to be implemented)

## Core Domain Rules (CRITICAL)
- **Use `Session`, not `Meeting`:** The core data model MUST be built around a generic `Session` entity. Do not hardcode tables, classes, or structs as `Meeting`.
  - A `Session` can represent a meeting, voice note, lecture, or imported file. The specific kind is just an attribute (`type`).
- **Local-First & Privacy:** All user data (audio, transcripts, summaries) MUST remain on the local machine. Do not use external cloud databases.
- **AI Agnostic:** Any AI integration (LLM, Speech-to-Text) must be built against generic interfaces to easily swap between local (e.g., Ollama, Whisper.cpp) and remote (e.g., OpenAI, Anthropic) providers.

## Standard Development Commands
*(Note: Ensure you are in the project root)*
- **Run Development App:** `npm run tauri dev`
- **Build Release App:** `npm run tauri build`
- **Rust Linting:** `cargo clippy --manifest-path src-tauri/Cargo.toml`
- **Rust Tests:** `cargo test --manifest-path src-tauri/Cargo.toml`
