# Postilla

**Capture. Understand. Remember.**

Postilla è un'applicazione desktop local-first per catturare, trascrivere, riassumere e organizzare conversazioni, riunioni, note vocali, lezioni e file audio.

Costruita con [Tauri 2](https://v2.tauri.app/), [React](https://react.dev/), [TypeScript](https://www.typescriptlang.org/) e [Rust](https://www.rust-lang.org/).

---

## Installazione

Scarica l'ultima release per il tuo sistema dalla pagina [Releases](https://github.com/fzanti/postilla/releases).

| Piattaforma | Formato |
|---|---|
| **Windows** | `.msi` o `.exe` |
| **macOS** | `.dmg` |
| **Ubuntu/Debian** | `.deb` |
| **Fedora** | `.rpm` |
| **Altre Linux** | `.AppImage` |

---

## Sviluppo

### Prerequisiti per piattaforma

<details>
<summary><b>Ubuntu / Debian</b></summary>

```bash
# Node.js 20+
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Dipendenze di sistema Tauri
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libsoup-3.0-dev \
  libxdo-dev \
  libssl-dev \
  build-essential \
  file
```
</details>

<details>
<summary><b>Fedora</b></summary>

```bash
# Node.js 20+
sudo dnf install -y nodejs

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Dipendenze di sistema Tauri
sudo dnf install -y \
  webkit2gtk4.1-devel \
  libxdo-devel \
  openssl-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  gcc-c++ \
  make \
  file \
  which \
  perl
```
</details>

<details>
<summary><b>Windows</b></summary>

1. **Node.js 20+** — Scarica da [nodejs.org](https://nodejs.org/) o via winget:
   ```powershell
   winget install OpenJS.NodeJS.LTS
   ```

2. **Rust** — Scarica da [rustup.rs](https://rustup.rs/) o via winget:
   ```powershell
   winget install Rustlang.Rustup
   ```

3. **WebView2** — Preinstallato su Windows 10+ (aggiornamento: `winget install Microsoft.WebView2Runtime`)

4. **Microsoft Visual Studio Build Tools** — Durante l'installazione di Rust, seleziona il componente *"Desktop development with C++"* oppure installa [Build Tools for Visual Studio 2022](https://visualstudio.microsoft.com/it/downloads/#build-tools-for-visual-studio-2022) con il workload *"Desktop development with C++"*.

5. **Git** — `winget install Git.Git`
</details>

<details>
<summary><b>macOS</b></summary>

1. **Xcode Command Line Tools:**
   ```bash
   xcode-select --install
   ```

2. **Node.js 20+** — Via [Homebrew](https://brew.sh/):
   ```bash
   brew install node@20
   ```

3. **Rust:**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source "$HOME/.cargo/env"
   ```

macOS non richiede dipendenze di sistema aggiuntive (WebView2 è nativo con WKWebView).
</details>

---

### Setup del progetto

```bash
# Clona il repository
git clone https://github.com/fzanti/postilla.git
cd postilla

# Installa le dipendenze frontend
npm install

# Avvia l'app in modalità sviluppo (Vite HMR + Tauri)
npm run tauri dev
```

> **Nota:** Il primo avvio compilerà anche il backend Rust, che può richiedere qualche minuto. Le ricompilazioni successive sono più veloci.

### Comandi utili

| Comando | Descrizione |
|---|---|
| `npm run tauri dev` | Avvia l'app in sviluppo con hot-reload |
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
│   │   ├── lib.rs          # Comandi Tauri (<40)
│   │   ├── db.rs           # Database SQLite (Session, FTS)
│   │   ├── transcribe.rs   # Trascrizione locale (Parakeet/Whisper)
│   │   ├── llm.rs          # Dispatch LLM generico
│   │   ├── remote_llm.rs   # Provider remoti (OpenAI, Anthropic)
│   │   └── license.rs      # Verifica licenza (Keygen.sh)
│   ├── Cargo.toml
│   ├── tauri.conf.json     # Configurazione Tauri
│   └── capabilities/       # Permessi (default.json, desktop.json)
├── .github/workflows/      # CI/CD (build su push/PR/tag)
├── public/                 # Asset statici
├── index.html              # HTML d'ingresso
├── vite.config.ts          # Configurazione Vite (porta 1420)
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
- **Rust:** Segui i warning di `clippy`, formatta con `rustfmt`
- **Commit:** Messaggi descrittivi in inglese, prefissi consigliati: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`
- **Versioni:** Aggiorna `package.json` e `Cargo.toml` rispettando il semantic versioning per le release

---

## Stack tecnologico

| Layer | Tecnologia |
|---|---|
| Frontend | React 19, TypeScript 5, Vite 7 |
| Backend | Rust, Tauri 2 |
| Database | SQLite (rusqlite, bundled) |
| Audio | wavesurfer.js 7 |
| UI Icons | lucide-react |
| CI/CD | GitHub Actions + Keygen.sh |
