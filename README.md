# Postilla

**Capture. Understand. Remember.**

Postilla è un'applicazione desktop local-first per catturare, trascrivere, riassumere e organizzare conversazioni, riunioni, note vocali, lezioni e file audio.

Costruita con [Tauri 2](https://v2.tauri.app/), [React](https://react.dev/), [TypeScript](https://www.typescriptlang.org/) e [Rust](https://www.rust-lang.org/).

---

## Prerequisiti

- **Node.js** 20+
- **Rust toolchain** (via [rustup](https://rustup.rs/))
- **Dipendenze di sistema Linux** (WebKit2GTK, librerie di sistema):

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev
```

> Per altri sistemi operativi consulta la [guida ufficiale Tauri](https://v2.tauri.app/start/prerequisites/).

---

## Sviluppo

```bash
# Clona il repository
git clone https://github.com/fzanti/postilla.git
cd postilla

# Installa le dipendenze frontend
npm install

# Avvia l'app in modalità sviluppo (Vite HMR + Tauri)
npm run tauri dev
```

L'app si avvierà con hot-reload sul frontend e ricarica automatica del backend Rust alla compilazione.

### Comandi utili

| Comando | Descrizione |
|---|---|
| `npm run tauri dev` | Avvia l'app in sviluppo |
| `npm run tauri build` | Compila il rilascio per la piattaforma corrente |
| `npm run dev` | Avvia solo il frontend Vite (porta 1420) |
| `npm run build` | Compila il frontend (TypeScript + Vite) |
| `cargo clippy` | Analisi statica del codice Rust |
| `cargo test` | Esegue i test Rust |

---

## Struttura del progetto

```
postilla/
├── src/                    # Frontend React + TypeScript
│   ├── App.tsx             # Componente principale
│   ├── App.css             # Stili (tema chiaro/scuro)
│   └── main.tsx            # Punto d'ingresso React
├── src-tauri/              # Backend Rust (Tauri)
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── lib.rs          # Comandi Tauri
│   │   ├── db.rs           # Database SQLite
│   │   ├── transcribe.rs   # Trascrizione locale
│   │   ├── llm.rs          # Dispatch LLM
│   │   ├── remote_llm.rs   # Provider remoti (OpenAI, Anthropic)
│   │   └── license.rs      # Verifica licenza
│   ├── Cargo.toml
│   └── tauri.conf.json     # Configurazione Tauri
├── public/                 # Asset statici
├── index.html              # HTML d'ingresso
├── vite.config.ts          # Configurazione Vite
├── package.json
└── tsconfig.json
```

---

## Linee guida per contribuire

### Principi architetturali

1. **Session, non Meeting** — Il modello dati è basato su `Session` (non `Meeting`). Una sessione può rappresentare una riunione, una nota vocale, una lezione o un file importato. Usa `session_type` per distinguerli.
2. **Local-first** — Tutti i dati (audio, trascrizioni, riassunti) restano sulla macchina locale. Nessun cloud esterno.
3. **AI-agnostico** — Le integrazioni AI (LLM, STT) usano interfacce generiche per supportare più provider (Ollama, OpenAI, Anthropic, Whisper.cpp).

### Come contribuire

1. Fai un fork del repository
2. Crea un branch per la tua feature (`git checkout -b feat/nome-feature`)
3. Assicurati che il codice passi i controlli:
   ```bash
   # Frontend
   npm run build
   # Backend
   cargo clippy && cargo test
   ```
4. Fai una pull request verso `main`

### Convenzioni di codice

- **TypeScript:** Strict mode, no unused locals/parameters
- **Rust:** Segui i warning di `clippy`, formatta con `rust fmt`
- **Commit:** Messaggi descrittivi in inglese, prefissi suggeriti: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`
- **Package.json e Cargo.toml:** aggiorna le versioni rispettando il semantic versioning

---

## Licenza

Vedere il file `LICENSE` (se presente) per i dettagli.

---

## Stack tecnologico

| Layer | Tecnologia |
|---|---|
| Frontend | React 19, TypeScript, Vite |
| Backend | Rust, Tauri 2 |
| Database | SQLite (via rusqlite) |
| Audio | wavesurfer.js |
| UI Icons | lucide-react |
