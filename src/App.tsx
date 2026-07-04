import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { BaseDirectory, mkdir, writeFile } from '@tauri-apps/plugin-fs';
import { appDataDir, join } from '@tauri-apps/api/path';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { Play, Mic, FileAudio, Sparkles, Languages, Settings2, MicOff, RefreshCw, ChevronDown, Download } from "lucide-react";
import "./App.css";

interface Session {
  id: number;
  session_type: string;
  title: string;
  created_at: string;
  updated_at: string;
  status: string;
  file_path?: string;
  transcript?: string;
  summary?: string;
}

interface DownloadProgress {
  item: string;
  progress: number;
}

interface OllamaModel {
  name: string;
  score: number;
  recommended: boolean;
}

function App() {
  const [isLicensed, setIsLicensed] = useState<boolean>(false); // Set to false by default in production
  const [licenseKey, setLicenseKey] = useState("");
  const [licenseError, setLicenseError] = useState("");
  const [isVerifying, setIsVerifying] = useState(false);

  const [sessions, setSessions] = useState<Session[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<number | null>(null);
  
  const [newTitle, setNewTitle] = useState("");
  const [newType, setNewType] = useState("meeting");
  
  // Recording state
  const [recordingId, setRecordingId] = useState<number | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);

  const [transcribingId, setTranscribingId] = useState<number | null>(null);
  const [summarizingId, setSummarizingId] = useState<number | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const [transcribeLang, setTranscribeLang] = useState<string>("auto");

  // Ollama Models state
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([]);
  const [selectedLlm, setSelectedLlm] = useState<string>("");
  const [ollamaConnected, setOllamaConnected] = useState<boolean>(true);

  // Updater state
  const [updateAvailable, setUpdateAvailable] = useState<any>(null);
  const [isUpdating, setIsUpdating] = useState(false);

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      setDownloadProgress(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function loadSessions() {
    try {
      const data = await invoke<Session[]>("get_sessions");
      setSessions(data);
    } catch (err) {
      console.error("Failed to load sessions:", err);
    }
  }

  async function checkOllamaModels() {
    try {
      const models = await invoke<OllamaModel[]>("get_ollama_models");
      setOllamaModels(models);
      setOllamaConnected(true);
      if (models.length > 0 && !selectedLlm) {
        // Find recommended or fallback to first
        const recommended = models.find(m => m.recommended);
        setSelectedLlm(recommended ? recommended.name : models[0].name);
      }
    } catch (err) {
      console.error("Ollama not running or no models:", err);
      setOllamaConnected(false);
    }
  }

  async function checkForUpdates() {
    try {
      const update = await check();
      if (update?.available) {
        setUpdateAvailable(update);
      }
    } catch (err) {
      console.error("Failed to check for updates:", err);
    }
  }

  useEffect(() => {
    const checkLicense = async () => {
      const savedKey = localStorage.getItem("postilla_license");
      if (savedKey) {
        setIsLicensed(true);
      } else {
        setIsLicensed(false);
      }
    };
    checkLicense();
    
    if (isLicensed) {
      loadSessions();
      checkOllamaModels();
      checkForUpdates();
    }
  }, [isLicensed]);

  async function handleActivateLicense(e: React.FormEvent) {
    e.preventDefault();
    if (!licenseKey) return;

    setIsVerifying(true);
    setLicenseError("");

    try {
      const deviceId = await invoke<string>("get_device_id");
      const isValid = await invoke<boolean>("verify_license", { 
        licenseKey, 
        deviceId 
      });

      if (isValid) {
        localStorage.setItem("postilla_license", licenseKey);
        setIsLicensed(true);
      }
    } catch (err: any) {
      setLicenseError(err.message || err);
    } finally {
      setIsVerifying(false);
    }
  }

  async function handleUpdate() {
    if (!updateAvailable) return;
    try {
      setIsUpdating(true);
      await updateAvailable.downloadAndInstall();
      await relaunch();
    } catch (err) {
      console.error("Failed to update:", err);
      alert("Failed to install update.");
      setIsUpdating(false);
    }
  }

  async function handleCreateSession(e: React.FormEvent) {
    e.preventDefault();
    if (!newTitle) return;

    try {
      const newSession = await invoke<Session>("create_session", {
        sessionType: newType,
        title: newTitle,
      });
      setNewTitle("");
      await loadSessions();
      setSelectedSessionId(newSession.id);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  }

  async function handleImport(sessionId: number) {
    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Audio',
          extensions: ['mp3', 'wav', 'm4a', 'ogg', 'webm']
        }]
      });

      if (selected && typeof selected === 'string') {
        await invoke("import_audio", {
          sessionId,
          sourcePath: selected
        });
        await loadSessions();
      }
    } catch (err) {
      console.error("Failed to import audio:", err);
    }
  }

  async function startRecording(sessionId: number) {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mediaRecorder = new MediaRecorder(stream, { mimeType: 'audio/webm' });
      mediaRecorderRef.current = mediaRecorder;
      audioChunksRef.current = [];

      mediaRecorder.ondataavailable = (event) => {
        if (event.data && event.data.size > 0) {
          audioChunksRef.current.push(event.data);
        }
      };

      mediaRecorder.onstop = async () => {
        // Fallback mime type se non supportato in registrazione
        const audioBlob = new Blob(audioChunksRef.current, { type: mediaRecorder.mimeType || 'audio/webm' });
        
        if (audioBlob.size === 0) {
            console.error("Audio blob is empty!");
            return;
        }

        const arrayBuffer = await audioBlob.arrayBuffer();
        const uint8Array = new Uint8Array(arrayBuffer);
        
        try {
          // Salva tramite FS plugin invece che passare array giganti via IPC
          await mkdir('media', { baseDir: BaseDirectory.AppData, recursive: true });
          const fileName = `session_${sessionId}.webm`;
          const dirPath = await appDataDir();
          const targetPath = await join(dirPath, 'media', fileName);
          
          await writeFile(targetPath, uint8Array);

          await invoke("save_audio_recording", {
            sessionId,
            targetPathStr: targetPath
          });
          await loadSessions();
        } catch (err) {
          console.error("Failed to save recording:", err);
        }

        stream.getTracks().forEach(track => track.stop());
      };

      // Richiediamo i dati a chunk regolari di 1 secondo per forzare lo svuotamento del buffer
      mediaRecorder.start(1000);
      setRecordingId(sessionId);
    } catch (err: any) {
      console.error("Failed to start recording:", err);
      alert(`Could not access microphone: ${err.message || err}`);
    }
  }

  function stopRecording() {
    if (mediaRecorderRef.current && recordingId) {
      mediaRecorderRef.current.stop();
      setRecordingId(null);
    }
  }

  async function handleTranscribe(sessionId: number) {
    try {
      setTranscribingId(sessionId);
      setDownloadProgress({ item: "Initializing...", progress: 0 });
      await invoke("transcribe_session", { sessionId, language: transcribeLang });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to transcribe:", err);
      alert(`Transcription failed: ${err.message || err}`);
    } finally {
      setTranscribingId(null);
      setDownloadProgress(null);
    }
  }

  async function handleSummarize(sessionId: number) {
    if (!selectedLlm) {
      alert("Nessun modello Ollama selezionato o disponibile.");
      return;
    }
    
    try {
      setSummarizingId(sessionId);
      await invoke("summarize_session", { sessionId, model: selectedLlm });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to summarize:", err);
      alert(`Summarization failed: ${err.message || err}\n\nAssicurati che Ollama sia avviato.`);
    } finally {
      setSummarizingId(null);
    }
  }

  const selectedSession = sessions.find(s => s.id === selectedSessionId);

  if (!isLicensed) {
    return (
      <div className="license-screen">
        <div className="license-card">
          <div className="empty-state-icon">
            <Sparkles size={40} color="#8b5cf6" />
          </div>
          <h2>Activate Postilla</h2>
          <p>Please enter your license key to unlock the app. You can find it in your purchase email.</p>
          
          <form onSubmit={handleActivateLicense}>
            <input
              type="text"
              className="plaud-input"
              placeholder="e.g. POSTILLA-PRO-123"
              value={licenseKey}
              onChange={e => setLicenseKey(e.target.value)}
              autoFocus
              style={{ textAlign: 'center', letterSpacing: '2px', fontFamily: 'monospace' }}
            />
            {licenseError && <div className="error-text mt-1">{licenseError}</div>}
            
            <button 
              type="submit" 
              className="plaud-btn btn-primary" 
              disabled={isVerifying || !licenseKey}
              style={{ width: '100%', marginTop: '1.5rem' }}
            >
              {isVerifying ? 'Verifying...' : 'Activate License'}
            </button>
          </form>

          <div className="license-footer">
            Don't have a license? <a href="https://app.nanocorp.so/" target="_blank" rel="noreferrer">Buy one now</a>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app-layout">
      <main className="main-content">
        {selectedSession ? (
          <div className="detail-view">
            <header className="detail-header">
              <h1>{selectedSession.title}</h1>
              <div className="detail-meta">
                <span className="pill">{selectedSession.session_type}</span>
                <span className="date">{new Date(selectedSession.created_at).toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit' })}</span>
              </div>
            </header>

            <div className="detail-body">
              {!selectedSession.file_path && (
                <div className="card action-card">
                  <div className="action-card-content">
                    <div className="action-circle">
                      <Mic size={32} color="#007aff" />
                    </div>
                    <h3>Ready to capture</h3>
                    <p>Start recording your voice or import an existing audio file.</p>
                    <div className="actions-center">
                      {recordingId === selectedSession.id ? (
                        <button className="plaud-btn btn-danger active" onClick={stopRecording}>
                          <MicOff size={18} /> Stop Recording
                        </button>
                      ) : (
                        <button className="plaud-btn btn-primary" onClick={() => startRecording(selectedSession.id)} disabled={recordingId !== null}>
                          <Mic size={18} /> Record Audio
                        </button>
                      )}
                      <button className="plaud-btn btn-outline" onClick={() => handleImport(selectedSession.id)} disabled={recordingId !== null}>
                        <FileAudio size={18} /> Import File
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {selectedSession.file_path && (
                <div className="card media-card">
                  <div className="media-info">
                    <div className="media-icon">
                      <Play size={20} color="#1d1d1f" />
                    </div>
                    <div className="media-details">
                      <strong>Audio File</strong>
                      <span>{selectedSession.file_path.split('/').pop() || selectedSession.file_path.split('\\').pop()}</span>
                    </div>
                  </div>
                  
                  {!selectedSession.transcript && (
                     <div className="transcribe-action">
                       <div className="select-wrapper">
                         <Languages className="select-icon-left" size={16} />
                         <select className="plaud-select lang-select" value={transcribeLang} onChange={e => setTranscribeLang(e.target.value)}>
                           <option value="auto">Auto-detect Language</option>
                           <option value="it">Italian (it)</option>
                           <option value="en">English (en)</option>
                           <option value="fr">French (fr)</option>
                           <option value="es">Spanish (es)</option>
                           <option value="de">German (de)</option>
                         </select>
                         <ChevronDown className="select-icon" size={16} />
                       </div>
                       <button 
                         className="plaud-btn btn-accent" 
                         onClick={() => handleTranscribe(selectedSession.id)}
                         disabled={transcribingId !== null}
                       >
                         {transcribingId === selectedSession.id ? <><RefreshCw size={18} className="spin" /> Transcribing...</> : <><Sparkles size={18} /> Transcribe</>}
                       </button>
                     </div>
                  )}
                  
                  {transcribingId === selectedSession.id && downloadProgress && (
                    <div className="progress-container">
                      <div className="progress-text">
                        {downloadProgress.item} ({Math.round(downloadProgress.progress)}%)
                      </div>
                      <div className="progress-bar">
                        <div 
                          className="progress-fill" 
                          style={{ width: `${downloadProgress.progress}%` }}
                        ></div>
                      </div>
                    </div>
                  )}
                </div>
              )}

              {selectedSession.transcript && (
                <div className="card transcript-card">
                  <div className="card-header">
                    <h3>Transcript</h3>
                    {!selectedSession.summary && (
                      <div className="llm-action">
                        {!ollamaConnected ? (
                           <span className="error-text text-small">Ollama not running</span>
                        ) : (
                          <>
                            <div className="select-wrapper">
                              <Settings2 className="select-icon-left" size={16} />
                              <select 
                                className="plaud-select llm-select" 
                                value={selectedLlm} 
                                onChange={e => setSelectedLlm(e.target.value)}
                                title="Select LLM for summary"
                              >
                                {ollamaModels.map(m => (
                                  <option key={m.name} value={m.name}>
                                    {m.name} {m.recommended ? '⭐' : ''}
                                  </option>
                                ))}
                              </select>
                              <ChevronDown className="select-icon" size={16} />
                            </div>
                            <button 
                               className="plaud-btn btn-primary btn-small"
                               onClick={() => handleSummarize(selectedSession.id)}
                               disabled={summarizingId !== null || !selectedLlm}
                            >
                               {summarizingId === selectedSession.id ? <><RefreshCw size={14} className="spin" /> Summarizing...</> : <><Sparkles size={14} /> Summarize</>}
                            </button>
                          </>
                        )}
                      </div>
                    )}
                  </div>
                  <div className="transcript-text">
                    {selectedSession.transcript}
                  </div>
                </div>
              )}

              {selectedSession.summary && (
                <div className="card summary-card">
                  <div className="card-header">
                    <h3 className="gradient-text"><Sparkles size={18} /> AI Summary & Action Items</h3>
                    <div className="llm-action">
                      <div className="select-wrapper">
                        <select 
                          className="plaud-select llm-select" 
                          value={selectedLlm} 
                          onChange={e => setSelectedLlm(e.target.value)}
                        >
                          {ollamaModels.map(m => (
                            <option key={m.name} value={m.name}>
                              {m.name} {m.recommended ? '⭐' : ''}
                            </option>
                          ))}
                        </select>
                        <ChevronDown className="select-icon" size={16} />
                      </div>
                      <button 
                         className="plaud-btn btn-outline btn-small"
                         onClick={() => handleSummarize(selectedSession.id)}
                         disabled={summarizingId !== null || !selectedLlm}
                      >
                         {summarizingId === selectedSession.id ? <><RefreshCw size={14} className="spin" /> Summarizing...</> : <><RefreshCw size={14} /> Retry</>}
                      </button>
                    </div>
                  </div>
                  <div className="summary-text">
                    {selectedSession.summary.split('\n').map((line, i) => (
                      <span key={i}>
                        {line}
                        <br />
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="empty-state">
            <div className="empty-state-icon">
              <Mic size={48} color="#007aff" strokeWidth={1.5} />
            </div>
            <h2>Capture your thoughts</h2>
            <p>Select a session or create a new one to start recording and transcribing.</p>
            
            <form className="create-form card" onSubmit={handleCreateSession}>
              <div className="form-group">
                <input
                  id="title-input"
                  className="plaud-input"
                  onChange={(e) => setNewTitle(e.currentTarget.value)}
                  placeholder="New recording title..."
                  value={newTitle}
                  autoFocus
                />
              </div>
              <div className="form-group row">
                <div className="select-wrapper">
                  <select className="plaud-select" value={newType} onChange={(e) => setNewType(e.currentTarget.value)}>
                    <option value="meeting">Meeting</option>
                    <option value="voice_note">Voice Note</option>
                    <option value="lecture">Lecture</option>
                    <option value="import">Imported File</option>
                  </select>
                  <ChevronDown className="select-icon" size={16} />
                </div>
                <button className="plaud-btn btn-primary" type="submit">Create</button>
              </div>
            </form>
          </div>
        )}
      </main>

      <aside className="sidebar-right">
        <div className="sidebar-header">
          <h2>Sessions</h2>
          <button className="mac-icon-btn" onClick={() => setSelectedSessionId(null)} title="New Session">
            ➕
          </button>
        </div>

        {updateAvailable && (
          <div className="update-banner">
            <div className="update-info">
              <Download size={16} />
              <span>Update available ({updateAvailable.version})</span>
            </div>
            <button className="plaud-btn btn-primary btn-small" onClick={handleUpdate} disabled={isUpdating}>
              {isUpdating ? 'Updating...' : 'Install'}
            </button>
          </div>
        )}
        
        <div className="sidebar-list">
          {sessions.length === 0 ? (
            <p className="sidebar-empty">No sessions yet.</p>
          ) : (
            sessions.map((session) => (
              <div 
                key={session.id} 
                className={`sidebar-item ${selectedSessionId === session.id ? 'active' : ''}`}
                onClick={() => setSelectedSessionId(session.id)}
              >
                <h4>{session.title}</h4>
                <div className="sidebar-item-meta">
                  <span className="type">{session.session_type}</span>
                  <span className="time">{new Date(session.created_at).toLocaleDateString()}</span>
                </div>
              </div>
            ))
          )}
        </div>
      </aside>
    </div>
  );
}

export default App;
