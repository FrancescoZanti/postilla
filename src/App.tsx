import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { BaseDirectory, mkdir, writeFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { appDataDir, join } from '@tauri-apps/api/path';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { Play, Mic, FileAudio, Sparkles, Languages, Settings2, MicOff, RefreshCw, ChevronDown, Download, Pencil, BrainCircuit, TreePine, X, Users, Pause } from "lucide-react";
import WaveSurfer from 'wavesurfer.js';
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
  mind_map?: string;
  template_id?: number | null;
  participants?: string | null;
  tags?: string | null;
}

interface Template {
  id: number;
  name: string;
  session_type: string;
  system_prompt: string;
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

interface ExportTemplate {
  id: number;
  name: string;
  body: string;
}

const SESSION_TYPE_LABELS: Record<string, string> = {
  meeting: "Meeting",
  voice_note: "Voice Note",
  lecture: "Lecture",
  import: "Imported File",
};

const SESSION_TYPE_OPTIONS = [
  { value: "meeting", label: "Meeting" },
  { value: "voice_note", label: "Voice Note" },
  { value: "lecture", label: "Lecture" },
  { value: "import", label: "Imported File" },
];

function AudioPlayer({ filePath }: { filePath: string }) {
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WaveSurfer | null>(null);
  const blobUrlRef = useRef<string | null>(null);
  const [ready, setReady] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setReady(false);
    wsRef.current?.destroy();
    wsRef.current = null;
    setPlaying(false);
    if (blobUrlRef.current) { URL.revokeObjectURL(blobUrlRef.current); blobUrlRef.current = null; }
    (async () => {
      try {
        const { mime, b64 } = await invoke<{ mime: string; b64: string }>("read_audio_file", { path: filePath });
        if (cancelled) return;
        const binary = atob(b64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
        const blob = new Blob([bytes], { type: mime });
        const url = URL.createObjectURL(blob);
        blobUrlRef.current = url;
        if (cancelled) { URL.revokeObjectURL(url); return; }

        const ws = WaveSurfer.create({
          container: containerRef.current!,
          waveColor: '#c0c0c8',
          progressColor: '#007aff',
          cursorColor: '#007aff',
          cursorWidth: 1,
          barWidth: 2,
          barGap: 1,
          barRadius: 2,
          height: 48,
          backend: 'MediaElement',
        });
        ws.load(url);
        ws.on('ready', () => { if (!cancelled) { setLoading(false); setReady(true); ws.setPlaybackRate(playbackRate); } });
        ws.on('finish', () => { if (!cancelled) setPlaying(false); });
        ws.on('error', (e) => { if (!cancelled) setError(String(e)); });
        wsRef.current = ws;
      } catch (err: any) {
        if (!cancelled) setError(err.message || String(err));
      }
    })();
    return () => { cancelled = true; };
  }, [filePath]);

  useEffect(() => {
    wsRef.current?.setPlaybackRate(playbackRate);
  }, [playbackRate]);

  function togglePlay() {
    const ws = wsRef.current;
    if (!ws) return;
    if (ws.isPlaying()) { ws.pause(); setPlaying(false); }
    else { ws.play(); setPlaying(true); }
  }

  if (error) return <div className="error-text">Audio error: {error}</div>;
  return (
    <div className="waveform-player">
      <button className="waveform-play-btn" onClick={togglePlay} disabled={loading || !ready}>
        {loading ? <RefreshCw size={16} className="spin" /> : playing ? <Pause size={16} /> : <Play size={16} />}
      </button>
      <div ref={containerRef} className="waveform-container" />
      <div className="playback-speed">
        {[0.5, 0.75, 1, 1.25, 1.5, 2].map(rate => (
          <button key={rate} className={`speed-btn ${playbackRate === rate ? 'active' : ''}`}
            onClick={() => setPlaybackRate(rate)}>{rate}x</button>
        ))}
      </div>
    </div>
  );
}

// Render text with **Speaker:** patterns as styled labels
function renderAnnotated(text: string) {
  const parts = text.split(/(\*\*[^*]+\*\*:?\s*)/);
  return parts.map((part, i) => {
    const match = part.match(/^\*\*([^*]+)\*\*:?\s*$/);
    if (match) {
      return <strong key={i} className="speaker-label">{match[1]}</strong>;
    }
    return <span key={i}>{part}</span>;
  });
}

// Convert basic markdown to HTML for PDF print view
function renderMarkdownAsHtml(md: string): string {
  let html = md
    // Headers
    .replace(/^### (.+)$/gm, '<h3>$1</h3>')
    .replace(/^## (.+)$/gm, '<h2>$1</h2>')
    .replace(/^# (.+)$/gm, '<h1>$1</h1>')
    // Bold
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    // Horizontal rules
    .replace(/^---$/gm, '<hr>')
    // Unordered list items
    .replace(/^- (.+)$/gm, '<li>$1</li>')
    // Clean multiple <li> into <ul>
    .replace(/(<li>.*<\/li>\n?)+/g, '<ul>$&</ul>')
    // Paragraphs (double newlines)
    .replace(/\n\n/g, '</p><p>');
  return `<p>${html}</p>`;
}

function App() {
  const [isLicensed, setIsLicensed] = useState<boolean>(false);
  const [licenseKey, setLicenseKey] = useState("");
  const [licenseError, setLicenseError] = useState("");
  const [isVerifying, setIsVerifying] = useState(false);

  const [sessions, setSessions] = useState<Session[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<number | null>(null);

  // Recording state
  const [recordingId, setRecordingId] = useState<number | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);

  // Audio source selection
  const [audioDevices, setAudioDevices] = useState<MediaDeviceInfo[]>([]);
  const [selectedAudioDeviceId, setSelectedAudioDeviceId] = useState<string>(() => localStorage.getItem("postilla_audio_device") || "default");
  const [captureSystemAudio, setCaptureSystemAudio] = useState<boolean>(() => localStorage.getItem("postilla_capture_system_audio") === "true");
  const [systemAudioDeviceId, setSystemAudioDeviceId] = useState<string>(() => localStorage.getItem("postilla_system_audio_device") || "");
  const audioContextRef = useRef<AudioContext | null>(null);
  const recordingTracksRef = useRef<MediaStreamTrack[]>([]);

  const [transcribingId, setTranscribingId] = useState<number | null>(null);
  const [summarizingId, setSummarizingId] = useState<number | null>(null);
  const [mindMappingId, setMindMappingId] = useState<number | null>(null);
  const [titlingId, setTitlingId] = useState<number | null>(null);
  const [annotatingId, setAnnotatingId] = useState<number | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const [transcribeLang, setTranscribeLang] = useState<string>("auto");

  // Search
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Session[] | null>(null);
  const searchTimeout = useRef<number | null>(null);

  // Audio duration cache
  const [durations, setDurations] = useState<Record<number, string>>({});

  // Collapsible sections in detail view
  const [collapsedSections, setCollapsedSections] = useState<Set<string>>(new Set());

  function toggleSection(name: string) {
    setCollapsedSections(prev => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  // Inline editing state
  const [editingFields, setEditingFields] = useState<Record<number, { transcript?: boolean; summary?: boolean }>>({});

  // Ollama Models & LAN discovery
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([]);
  const [selectedLlm, setSelectedLlm] = useState<string>("");
  const [ollamaConnected, setOllamaConnected] = useState<boolean>(true);
  const [ollamaInstances, setOllamaInstances] = useState<{url: string; label: string; is_local: boolean}[]>([]);
  const [ollamaIsLocal, setOllamaIsLocal] = useState<boolean>(true);
  const [isDiscovering, setIsDiscovering] = useState(false);

  // Provider & API keys
  const [provider, setProvider] = useState<string>("ollama");
  const [openaiKey, setOpenaiKey] = useState("");
  const [anthropicKey, setAnthropicKey] = useState("");
  const [remoteModels, setRemoteModels] = useState<string[]>([]);

  // Templates
  const [templates, setTemplates] = useState<Template[]>([]);
  const [selectedTemplateId, setSelectedTemplateId] = useState<number | null>(null);

  // Updater
  const [updateAvailable, setUpdateAvailable] = useState<any>(null);
  const [isUpdating, setIsUpdating] = useState(false);

  // Settings view
  const [showSettings, setShowSettings] = useState(false);

  // Theme
  const [theme, setTheme] = useState(() => localStorage.getItem("postilla_theme") || "light");

  // Session comparison
  const [compareMode, setCompareMode] = useState(false);
  const [compareIds, setCompareIds] = useState<[number | null, number | null]>([null, null]);

  // Onboarding
  const [showOnboarding, setShowOnboarding] = useState(() => {
    return !localStorage.getItem("postilla_onboarding_done");
  });

  // Template editing in settings
  const [editingTemplate, setEditingTemplate] = useState<Template | null>(null);
  const [editName, setEditName] = useState("");
  const [editType, setEditType] = useState("meeting");
  const [editPrompt, setEditPrompt] = useState("");

  // Export templates
  const [exportTemplates, setExportTemplates] = useState<ExportTemplate[]>([]);
  const [editingExportTemplate, setEditingExportTemplate] = useState<ExportTemplate | null>(null);
  const [editExportName, setEditExportName] = useState("");
  const [editExportBody, setEditExportBody] = useState("");

  // RAG
  const [showRag, setShowRag] = useState(false);
  const [ragQuery, setRagQuery] = useState("");
  const [ragAnswer, setRagAnswer] = useState<string | null>(null);
  const [ragLoading, setRagLoading] = useState(false);

  // Auto-title: track which sessions had title auto-generated
  const autoTitleDone = useRef<Set<number>>(new Set());

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      setDownloadProgress(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const handler = () => loadAudioDevices();
    navigator.mediaDevices.addEventListener('devicechange', handler);
    loadAudioDevices();
    return () => navigator.mediaDevices.removeEventListener('devicechange', handler);
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("postilla_theme", theme);
  }, [theme]);

  async function loadSessions() {
    try {
      const data = await invoke<Session[]>("get_sessions");
      setSessions(data);
      // Load durations for sessions with audio files
      const durMap: Record<number, string> = {};
      await Promise.all(data.filter(s => s.file_path).map(async (s) => {
        try {
          const secs = await invoke<number>("get_audio_duration", { path: s.file_path! });
          const m = Math.floor(secs / 60);
          const sec = Math.floor(secs % 60);
          durMap[s.id] = `${m}:${sec.toString().padStart(2, '0')}`;
        } catch (_) {}
      }));
      setDurations(durMap);
    } catch (err) {
      console.error("Failed to load sessions:", err);
    }
  }

  async function loadTemplates() {
    try {
      const data = await invoke<Template[]>("get_templates");
      setTemplates(data);
    } catch (err) {
      console.error("Failed to load templates:", err);
    }
  }

  async function loadExportTemplates() {
    try {
      const data = await invoke<ExportTemplate[]>("get_export_templates");
      setExportTemplates(data);
    } catch (err) {
      console.error("Failed to load export templates:", err);
    }
  }

  async function checkOllamaModels() {
    try {
      const models = await invoke<OllamaModel[]>("get_ollama_models");
      setOllamaModels(models);
      setOllamaConnected(true);
      setOllamaIsLocal(true);
      if (models.length > 0 && !selectedLlm) {
        const recommended = models.find(m => m.recommended);
        setSelectedLlm(recommended ? recommended.name : models[0].name);
      }
    } catch (err) {
      console.error("Ollama not running on localhost:", err);
      setOllamaConnected(false);
      // Try to discover LAN instances
      discoverLanOllama();
    }
  }

  async function discoverLanOllama() {
    setIsDiscovering(true);
    try {
      const instances = await invoke<{url: string; label: string; is_local: boolean}[]>("discover_ollama");
      setOllamaInstances(instances);
      if (instances.length > 0) {
        // Auto-select the first LAN instance
        await selectLanOllamaInstance(instances[0].url);
      }
    } catch (err) {
      console.error("LAN discovery failed:", err);
    } finally {
      setIsDiscovering(false);
    }
  }

  async function selectLanOllamaInstance(url: string) {
    try {
      await invoke("set_ollama_url", { url });
      setOllamaIsLocal(false);
      // Refresh models from the new URL
      const models = await invoke<OllamaModel[]>("get_ollama_models");
      setOllamaModels(models);
      setOllamaConnected(true);
      if (models.length > 0 && !selectedLlm) {
        const recommended = models.find(m => m.recommended);
        setSelectedLlm(recommended ? recommended.name : models[0].name);
      }
    } catch (err) {
      console.error("Failed to connect to LAN Ollama:", err);
    }
  }

  async function checkForUpdates() {
    try {
      const update = await check();
      if (update?.available) setUpdateAvailable(update);
    } catch (err) {
      console.error("Failed to check for updates:", err);
    }
  }

  async function fetchRemoteModels(prov: string, key: string) {
    if (!key) { setRemoteModels([]); return; }
    try {
      const models = await invoke<string[]>("get_remote_models", { provider: prov, apiKey: key });
      setRemoteModels(models);
      if (models.length > 0 && !selectedLlm) {
        setSelectedLlm(models[0]);
      }
    } catch (err) {
      console.error(`Failed to fetch ${prov} models:`, err);
      setRemoteModels([]);
    }
  }

  function handleProviderChange(prov: string) {
    setProvider(prov);
    localStorage.setItem("postilla_provider", prov);
    if (prov === "openai" && openaiKey) fetchRemoteModels(prov, openaiKey);
    if (prov === "anthropic" && anthropicKey) fetchRemoteModels(prov, anthropicKey);
  }

  async function handleBackup() {
    try {
      const dest = await save({ filters: [{ name: 'SQLite Database', extensions: ['db', 'sqlite', 'sqlite3'] }] });
      if (!dest) return;
      await invoke("backup_database", { destPath: dest });
      alert("Database backed up successfully!");
    } catch (err: any) {
      alert("Backup failed: " + (err.message || err));
    }
  }

  async function handleRestore() {
    const confirmed = confirm("Restore will overwrite all current data. This cannot be undone. Continue?");
    if (!confirmed) return;
    try {
      const src = await open({ filters: [{ name: 'SQLite Database', extensions: ['db', 'sqlite', 'sqlite3'] }], multiple: false });
      if (!src) return;
      await invoke("restore_database", { sourcePath: src });
      alert("Database restored! Reloading...");
      window.location.reload();
    } catch (err: any) {
      alert("Restore failed: " + (err.message || err));
    }
  }

  function handleOpenaiKeyChange(key: string) {
    setOpenaiKey(key);
    localStorage.setItem("postilla_openai_key", key);
    if (key) fetchRemoteModels("openai", key);
  }

  function handleAnthropicKeyChange(key: string) {
    setAnthropicKey(key);
    localStorage.setItem("postilla_anthropic_key", key);
    if (key) fetchRemoteModels("anthropic", key);
  }

  // Get current active model list and effective API key
  function getModelOptions(): { name: string; recommended: boolean }[] {
    if (provider === "ollama") return ollamaModels;
    return remoteModels.map(m => ({ name: m, recommended: false }));
  }

  function getApiKey(): string {
    if (provider === "openai") return openaiKey;
    if (provider === "anthropic") return anthropicKey;
    return "";
  }
  function getDefaultTemplate(sessionType: string): Template | undefined {
    return templates.find(t => t.session_type === sessionType);
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
      loadTemplates();
      loadExportTemplates();
      checkOllamaModels();
      checkForUpdates();
      // Load saved API keys
      const savedOpenai = localStorage.getItem("postilla_openai_key") || "";
      const savedAnthropic = localStorage.getItem("postilla_anthropic_key") || "";
      const savedProvider = localStorage.getItem("postilla_provider") || "ollama";
      setOpenaiKey(savedOpenai);
      setAnthropicKey(savedAnthropic);
      setProvider(savedProvider);
    }
  }, [isLicensed]);

  // When session type changes, auto-select matching template
  useEffect(() => {
    if (selectedSessionId) {
      const session = sessions.find(s => s.id === selectedSessionId);
      if (session) {
        const tpl = getDefaultTemplate(session.session_type);
        if (tpl) setSelectedTemplateId(tpl.id);
      }
    }
  }, [selectedSessionId, sessions, templates]);

  async function handleActivateLicense(e: React.FormEvent) {
    e.preventDefault();
    if (!licenseKey) return;
    setIsVerifying(true);
    setLicenseError("");
    try {
      const deviceId = await invoke<string>("get_device_id");
      const isValid = await invoke<boolean>("verify_license", { licenseKey, deviceId });
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

  // Create session by button click (from empty state)
  async function quickCreateSession(type: string) {
    try {
      const s = await invoke<Session>("create_session", { sessionType: type, participants: null });
      await loadSessions();
      setSelectedSessionId(s.id);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  }

  async function handleImport(sessionId: number) {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'm4a', 'ogg', 'webm'] }]
      });
      if (selected && typeof selected === 'string') {
        await invoke("import_audio", { sessionId, sourcePath: selected });
        await loadSessions();
      }
    } catch (err) {
      console.error("Failed to import audio:", err);
    }
  }

  async function startRecording(sessionId: number) {
    try {
      const tracksToStop: MediaStreamTrack[] = [];
      let finalStream: MediaStream;

      const useMultiSource = captureSystemAudio && systemAudioDeviceId && systemAudioDeviceId !== selectedAudioDeviceId;

      if (useMultiSource) {
        const micConstraints = selectedAudioDeviceId === "default" || !selectedAudioDeviceId
          ? { audio: true }
          : { audio: { deviceId: { exact: selectedAudioDeviceId } } };
        const micStream = await navigator.mediaDevices.getUserMedia(micConstraints);
        tracksToStop.push(...micStream.getTracks());

        const sysStream = await navigator.mediaDevices.getUserMedia({
          audio: { deviceId: { exact: systemAudioDeviceId } }
        });
        tracksToStop.push(...sysStream.getTracks());

        const audioContext = new AudioContext();
        audioContextRef.current = audioContext;
        const dest = audioContext.createMediaStreamDestination();

        const micSource = audioContext.createMediaStreamSource(micStream);
        micSource.connect(dest);

        const sysSource = audioContext.createMediaStreamSource(sysStream);
        sysSource.connect(dest);

        finalStream = dest.stream;
      } else {
        const constraints = selectedAudioDeviceId === "default" || !selectedAudioDeviceId
          ? { audio: true }
          : { audio: { deviceId: { exact: selectedAudioDeviceId } } };
        const stream = await navigator.mediaDevices.getUserMedia(constraints);
        tracksToStop.push(...stream.getTracks());
        finalStream = stream;
      }

      recordingTracksRef.current = tracksToStop;

      const types = [
        'audio/webm;codecs=opus',
        'audio/webm',
        'audio/ogg;codecs=opus',
        'audio/ogg',
        'audio/mp4;codecs=mp4a.40.2',
        'audio/mp4',
        'audio/wav',
      ];
      let mimeType = '';
      for (const t of types) {
        if (MediaRecorder.isTypeSupported(t)) {
          mimeType = t;
          break;
        }
      }
      const options = mimeType ? { mimeType } : {};

      const mediaRecorder = new MediaRecorder(finalStream, options);
      mediaRecorderRef.current = mediaRecorder;
      audioChunksRef.current = [];

      mediaRecorder.ondataavailable = (event) => {
        if (event.data && event.data.size > 0) {
          audioChunksRef.current.push(event.data);
        }
      };

      mediaRecorder.onstop = async () => {
        const effectiveMime = mediaRecorder.mimeType || mimeType || 'audio/webm';
        const audioBlob = new Blob(audioChunksRef.current, { type: effectiveMime });
        if (audioBlob.size === 0) { console.error("Audio blob is empty!"); return; }

        const arrayBuffer = await audioBlob.arrayBuffer();
        const uint8Array = new Uint8Array(arrayBuffer);

        try {
          await mkdir('media', { baseDir: BaseDirectory.AppData, recursive: true });
          const ext = effectiveMime.includes('ogg') ? 'ogg'
            : effectiveMime.includes('mp4') ? 'm4a'
            : effectiveMime.includes('wav') ? 'wav'
            : 'webm';
          const fileName = `session_${sessionId}.${ext}`;
          const dirPath = await appDataDir();
          const targetPath = await join(dirPath, 'media', fileName);
          await writeFile(targetPath, uint8Array);
          await invoke("save_audio_recording", { sessionId, targetPathStr: targetPath });
          await loadSessions();
        } catch (err) {
          console.error("Failed to save recording:", err);
        }

        recordingTracksRef.current.forEach(track => track.stop());
        recordingTracksRef.current = [];

        if (audioContextRef.current) {
          audioContextRef.current.close().catch(() => {});
          audioContextRef.current = null;
        }
      };

      mediaRecorder.start(1000);
      setRecordingId(sessionId);
    } catch (err: any) {
      console.error("Failed to start recording:", err);
      alert(`Could not access audio: ${err.message || err}`);

      recordingTracksRef.current.forEach(track => track.stop());
      recordingTracksRef.current = [];
      if (audioContextRef.current) {
        audioContextRef.current.close().catch(() => {});
        audioContextRef.current = null;
      }
    }
  }

  function stopRecording() {
    if (mediaRecorderRef.current && recordingId) {
      mediaRecorderRef.current.stop();
      setRecordingId(null);
    }
  }

  async function loadAudioDevices() {
    try {
      await navigator.mediaDevices.getUserMedia({ audio: true }).then(stream =>
        stream.getTracks().forEach(t => t.stop())
      );
    } catch {
      // Permission not granted yet — that's OK
    }
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const audioInputs = devices.filter(d => d.kind === 'audioinput');
      setAudioDevices(audioInputs);

      if (!localStorage.getItem("postilla_system_audio_device")) {
        const monitor = audioInputs.find(d =>
          d.label.toLowerCase().includes('monitor') ||
          d.label.toLowerCase().includes('stereo mix') ||
          d.label.toLowerCase().includes('what u hear') ||
          d.label.toLowerCase().includes('loopback')
        );
        if (monitor) {
          setSystemAudioDeviceId(monitor.deviceId);
        } else if (audioInputs.length > 1) {
          setSystemAudioDeviceId(audioInputs[audioInputs.length - 1].deviceId);
        }
      }
    } catch (err) {
      console.error("Failed to enumerate audio devices:", err);
    }
  }

  async function handleTranscribe(sessionId: number) {
    try {
      setTranscribingId(sessionId);
      setDownloadProgress({ item: "Initializing...", progress: 0 });
      await invoke("transcribe_session", { sessionId, language: transcribeLang });
      await loadSessions();
      await autoGenerateTitle(sessionId);
    } catch (err: any) {
      console.error("Failed to transcribe:", err);
      alert(`Transcription failed: ${err.message || err}`);
    } finally {
      setTranscribingId(null);
      setDownloadProgress(null);
    }
  }

  async function runAutoPipeline(sessionId: number) {
    if (!selectedLlm) {
      alert("No Ollama model selected.");
      return;
    }

    // Step 1: Transcribe
    try {
      setTranscribingId(sessionId);
      setDownloadProgress({ item: "Initializing...", progress: 0 });
      await invoke("transcribe_session", { sessionId, language: transcribeLang });
      await loadSessions();
      await autoGenerateTitle(sessionId);
    } catch (err: any) {
      console.error("Pipeline failed at transcription:", err);
      alert(`Transcription failed: ${err.message || err}`);
      setTranscribingId(null);
      setDownloadProgress(null);
      return;
    }
    setTranscribingId(null);
    setDownloadProgress(null);

    // Step 2: Summarize
    try {
      setSummarizingId(sessionId);
      await invoke("summarize_session", {
        sessionId,
        provider,
        apiKey: getApiKey(),
        model: selectedLlm,
        templateId: selectedTemplateId,
      });
      await loadSessions();
      if (!autoTitleDone.current.has(sessionId)) {
        await autoGenerateTitle(sessionId);
      }
    } catch (err: any) {
      console.error("Pipeline failed at summarization:", err);
      alert(`Summarization failed: ${err.message || err}\n\nEnsure Ollama is running.`);
      setSummarizingId(null);
      return;
    }
    setSummarizingId(null);

    // Step 3: Mind map
    try {
      setMindMappingId(sessionId);
      await invoke("generate_mind_map", { sessionId, provider, apiKey: getApiKey(), model: selectedLlm });
      await loadSessions();
    } catch (err: any) {
      console.error("Pipeline failed at mind map:", err);
      alert(`Mind map generation failed: ${err.message || err}`);
    } finally {
      setMindMappingId(null);
    }
  }

  async function autoGenerateTitle(sessionId: number) {
    if (autoTitleDone.current.has(sessionId)) return;
    if (!selectedLlm) return;
    try {
      setTitlingId(sessionId);
      await invoke("generate_session_title", { sessionId, provider, apiKey: getApiKey(), model: selectedLlm });
      autoTitleDone.current.add(sessionId);
      await loadSessions();
    } catch {
      // Silent fail — title generation is optional
    } finally {
      setTitlingId(null);
    }
  }

  async function handleUpdateParticipants(sessionId: number, participants: string) {
    try {
      await invoke("update_session_participants", { sessionId, participants });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to update participants:", err);
    }
  }

  async function handleUpdateTags(sessionId: number, tags: string) {
    try {
      await invoke("update_session_tags", { sessionId, tags });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to update tags:", err);
    }
  }

  function startEditing(sessionId: number, field: 'transcript' | 'summary') {
    setEditingFields(prev => ({ ...prev, [sessionId]: { ...prev[sessionId], [field]: true } }));
  }

  function stopEditing(sessionId: number, field: 'transcript' | 'summary') {
    setEditingFields(prev => {
      const next = { ...prev };
      if (next[sessionId]) {
        next[sessionId] = { ...next[sessionId] };
        delete next[sessionId][field];
        if (Object.keys(next[sessionId]).length === 0) delete next[sessionId];
      }
      return next;
    });
  }

  async function saveTranscript(sessionId: number, value: string) {
    try {
      await invoke("update_session_transcript", { sessionId, transcript: value });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to save transcript:", err);
    }
    stopEditing(sessionId, 'transcript');
  }

  async function saveSummary(sessionId: number, value: string) {
    try {
      await invoke("update_session_summary", { sessionId, summary: value });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to save summary:", err);
    }
    stopEditing(sessionId, 'summary');
  }

  async function handleAnnotateSpeakers(sessionId: number) {
    if (!selectedLlm) { alert("No model selected."); return; }
    try {
      setAnnotatingId(sessionId);
      await invoke("annotate_speakers", { sessionId, provider, apiKey: getApiKey(), model: selectedLlm });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to annotate speakers:", err);
      alert(`Speaker annotation failed: ${err.message || err}`);
    } finally {
      setAnnotatingId(null);
    }
  }

  async function handleExportMarkdown(sessionId: number, title: string) {
    try {
      const md = await invoke<string>("export_session_markdown", { sessionId });
      const safeName = title.replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const path = await save({
        filters: [{ name: 'Markdown', extensions: ['md'] }],
        defaultPath: `${safeName}.md`,
      });
      if (path) {
        await writeTextFile(path, md);
      }
    } catch (err: any) {
      console.error("Export failed:", err);
      alert(`Export failed: ${err.message || err}`);
    }
  }

  async function handleExportPdf(sessionId: number, title: string) {
    try {
      const md = await invoke<string>("export_session_markdown", { sessionId });
      const safeName = title.replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const html = `<!DOCTYPE html>
<html><head><meta charset="utf-8">
  <title>${safeName}</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; max-width: 800px; margin: 40px auto; padding: 0 20px; line-height: 1.6; color: #1d1d1f; }
    h1 { font-size: 1.8rem; margin-bottom: 0.5rem; }
    h2 { font-size: 1.3rem; margin-top: 1.5rem; border-bottom: 1px solid #e5e5ea; padding-bottom: 0.3rem; }
    h3 { font-size: 1.1rem; }
    strong { color: #8b5cf6; }
    hr { border: none; border-top: 1px solid #e5e5ea; margin: 1.5rem 0; }
    .meta { color: #8e8e93; font-size: 0.9rem; }
    ul { padding-left: 1.5rem; }
    @media print { body { margin: 0; padding: 20px; } }
  </style>
</head><body>${renderMarkdownAsHtml(md)}</body></html>`;

      // Write to temporary file and open with system viewer for native print dialog
      const { appDataDir } = await import('@tauri-apps/api/path');
      const dir = await appDataDir();
      const tmpPath = await import('@tauri-apps/api/path').then(m => m.join(dir, 'export.html'));
      await writeTextFile(tmpPath, html);
      await import('@tauri-apps/plugin-opener').then(m => m.openPath(tmpPath));
    } catch (err: any) {
      console.error("PDF export failed:", err);
      alert(`PDF export failed: ${err.message || err}`);
    }
  }

  async function handleExportSrt(sessionId: number, title: string) {
    try {
      const srt = await invoke<string>("export_session_srt", { sessionId });
      const safeName = title.replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const path = await save({ filters: [{ name: 'SubRip', extensions: ['srt'] }], defaultPath: `${safeName}.srt` });
      if (path) await writeTextFile(path, srt);
    } catch (err: any) { console.error("SRT export failed:", err); alert(`Export failed: ${err.message || err}`); }
  }

  async function handleExportVtt(sessionId: number, title: string) {
    try {
      const vtt = await invoke<string>("export_session_vtt", { sessionId });
      const safeName = title.replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const path = await save({ filters: [{ name: 'WebVTT', extensions: ['vtt'] }], defaultPath: `${safeName}.vtt` });
      if (path) await writeTextFile(path, vtt);
    } catch (err: any) { console.error("VTT export failed:", err); alert(`Export failed: ${err.message || err}`); }
  }

  async function handleExportTxt(sessionId: number, title: string) {
    try {
      const txt = await invoke<string>("export_session_txt", { sessionId });
      const safeName = title.replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const path = await save({ filters: [{ name: 'Text', extensions: ['txt'] }], defaultPath: `${safeName}.txt` });
      if (path) await writeTextFile(path, txt);
    } catch (err: any) { console.error("TXT export failed:", err); alert(`Export failed: ${err.message || err}`); }
  }

  async function handleExportObsidian(sessionId: number, title: string) {
    try {
      const md = await invoke<string>("export_session_obsidian", { sessionId });
      const safeName = title.replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const path = await save({ filters: [{ name: 'Markdown', extensions: ['md'] }], defaultPath: `${safeName}.md` });
      if (path) await writeTextFile(path, md);
    } catch (err: any) { console.error("Obsidian export failed:", err); alert(`Export failed: ${err.message || err}`); }
  }

  async function handleDeleteSession(sessionId: number) {
    if (!confirm("Delete this session permanently? This action cannot be undone.")) return;
    try {
      await invoke("delete_session", { sessionId });
      if (selectedSessionId === sessionId) setSelectedSessionId(null);
      await loadSessions();
    } catch (err: any) {
      alert("Delete failed: " + (err.message || err));
    }
  }

  async function handleSummarize(sessionId: number) {
    if (!selectedLlm) {
      alert("No Ollama model selected.");
      return;
    }
    try {
      setSummarizingId(sessionId);
      await invoke("summarize_session", {
        sessionId,
        provider,
        apiKey: getApiKey(),
        model: selectedLlm,
        templateId: selectedTemplateId,
      });
      await loadSessions();
      // Auto-generate title if not already done
      if (!autoTitleDone.current.has(sessionId)) {
        await autoGenerateTitle(sessionId);
      }
    } catch (err: any) {
      console.error("Failed to summarize:", err);
      alert(`Summarization failed: ${err.message || err}\n\nEnsure Ollama is running.`);
    } finally {
      setSummarizingId(null);
    }
  }

  async function handleGenerateMindMap(sessionId: number) {
    if (!selectedLlm) {
      alert("No Ollama model selected.");
      return;
    }
    try {
      setMindMappingId(sessionId);
      await invoke("generate_mind_map", { sessionId, provider, apiKey: getApiKey(), model: selectedLlm });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to generate mind map:", err);
      alert(`Mind map generation failed: ${err.message || err}`);
    } finally {
      setMindMappingId(null);
    }
  }

  async function handleChangeSessionType(sessionId: number, newType: string) {
    try {
      await invoke("update_session_type", { sessionId, sessionType: newType });
      await loadSessions();
      // Auto-select template for new type
      const tpl = getDefaultTemplate(newType);
      if (tpl) setSelectedTemplateId(tpl.id);
    } catch (err: any) {
      console.error("Failed to update session type:", err);
    }
  }

  async function handleUpdateTitle(sessionId: number, newTitle: string) {
    try {
      await invoke("update_session_title", { sessionId, title: newTitle });
      await loadSessions();
    } catch (err: any) {
      console.error("Failed to update title:", err);
    }
  }

  // Template CRUD
  async function handleSaveTemplate() {
    if (!editName.trim() || !editPrompt.trim()) return;
    try {
      await invoke("save_template", {
        id: editingTemplate?.id ?? null,
        name: editName,
        sessionType: editType,
        systemPrompt: editPrompt,
      });
      await loadTemplates();
      cancelTemplateEdit();
    } catch (err: any) {
      console.error("Failed to save template:", err);
    }
  }

  async function handleDeleteTemplate(id: number) {
    try {
      await invoke("delete_template", { id });
      await loadTemplates();
    } catch (err: any) {
      console.error("Failed to delete template:", err);
    }
  }

  function startTemplateEdit(tpl: Template) {
    setEditingTemplate(tpl);
    setEditName(tpl.name);
    setEditType(tpl.session_type);
    setEditPrompt(tpl.system_prompt);
  }

  function startNewTemplate() {
    setEditingTemplate(null);
    setEditName("");
    setEditType("meeting");
    setEditPrompt("");
  }

  function cancelTemplateEdit() {
    setEditingTemplate(null);
    setEditName("");
    setEditType("meeting");
    setEditPrompt("");
  }

  // ──── Export Template management ────
  function startNewExportTemplate() {
    setEditingExportTemplate(null);
    setEditExportName("");
    setEditExportBody("# {title}\n\n- **Type:** {type}\n- **Date:** {date}\n- **Participants:** {participants}\n- **Tags:** {tags}\n\n---\n\n## Transcript\n\n{transcript}\n\n---\n\n## Summary\n\n{summary}\n\n---\n\n## Mind Map\n\n{mind_map}");
  }

  function editExportTemplate(t: ExportTemplate) {
    setEditingExportTemplate(t);
    setEditExportName(t.name);
    setEditExportBody(t.body);
  }

  async function saveExportTemplateFn() {
    if (!editExportName.trim()) return;
    try {
      await invoke<ExportTemplate>("save_export_template", {
        id: editingExportTemplate?.id ?? null,
        name: editExportName.trim(),
        body: editExportBody,
      });
      // Refresh list
      await loadExportTemplates();
      setEditingExportTemplate(null);
      setEditExportName("");
      setEditExportBody("");
    } catch (err: any) {
      console.error("Failed to save export template:", err);
    }
  }

  async function deleteExportTemplateFn(id: number) {
    if (!confirm("Delete this export template?")) return;
    try {
      await invoke("delete_export_template", { id });
      await loadExportTemplates();
    } catch (err: any) {
      console.error("Failed to delete export template:", err);
    }
  }

  async function handleExportWithTemplate(sessionId: number, templateId: number) {
    try {
      const md = await invoke<string>("export_session_with_template", { sessionId, templateId });
      const session = sessions.find(s => s.id === sessionId);
      const safeName = (session?.title || `session-${sessionId}`).replace(/[^a-zA-Z0-9-_ ]/g, '').trim() || `session-${sessionId}`;
      const path = await save({ filters: [{ name: 'Markdown', extensions: ['md'] }], defaultPath: `${safeName}.md` });
      if (path) await writeTextFile(path, md);
    } catch (err: any) {
      console.error("Export failed:", err);
    }
  }

  async function handleRagQuery() {
    if (!ragQuery.trim()) return;
    setRagLoading(true);
    setRagAnswer(null);
    try {
      const answer = await invoke<string>("rag_query", {
        question: ragQuery.trim(),
        provider,
        apiKey: getApiKey(),
        model: selectedLlm,
      });
      setRagAnswer(answer);
    } catch (err: any) {
      setRagAnswer("Error: " + (err.message || err));
    } finally {
      setRagLoading(false);
    }
  }

  const selectedSession = sessions.find(s => s.id === selectedSessionId);

  // Keyboard shortcuts
  useEffect(() => {
    function handleKey(e: KeyboardEvent) {
      const meta = e.metaKey || e.ctrlKey;
      const tag = (e.target as HTMLElement).tagName;
      if (meta && (tag === 'TEXTAREA' || tag === 'INPUT')) return;

      if (meta && e.key === 'n') {
        e.preventDefault();
        if (!showSettings) setSelectedSessionId(null);
      }
      if (meta && e.key === 'k') {
        e.preventDefault();
        document.querySelector<HTMLInputElement>('.sidebar-search input')?.focus();
      }
      if (meta && e.key === 'e' && selectedSession) {
        e.preventDefault();
        const tmpl = exportTemplates[0];
        if (tmpl) handleExportWithTemplate(selectedSession.id, tmpl.id);
      }
      if (e.key === 'Escape') {
        if (editingFields[selectedSession?.id ?? -1]?.transcript || editingFields[selectedSession?.id ?? -1]?.summary) return;
        if (showSettings) setShowSettings(false);
        else { setSearchQuery(""); setSearchResults(null); }
      }
    }
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [selectedSession, showSettings, editingFields]);

  // ──────── LICENSE SCREEN ────────
  if (!isLicensed) {
    return (
      <div className="license-screen">
        <div className="license-card">
          <div className="empty-state-icon"><Sparkles size={40} color="#8b5cf6" /></div>
          <h2>Activate Postilla</h2>
          <p>Please enter your license key to unlock the app.</p>
          <form onSubmit={handleActivateLicense}>
            <input type="text" className="plaud-input" placeholder="e.g. POSTILLA-PRO-123"
              value={licenseKey} onChange={e => setLicenseKey(e.target.value)}
              autoFocus style={{ textAlign: 'center', letterSpacing: '2px', fontFamily: 'monospace' }}
              aria-label="License key" />
            {licenseError && <div className="error-text mt-1">{licenseError}</div>}
            <button type="submit" className="plaud-btn btn-primary"
              disabled={isVerifying || !licenseKey} style={{ width: '100%', marginTop: '1.5rem' }}
              aria-label="Activate License">
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

  // ──────── SETTINGS VIEW ────────
  if (showSettings) {
    return (
      <div className="app-layout">
        <div className="settings-view">
          <header className="settings-header">
            <h2>Settings</h2>
            <button className="plaud-btn btn-outline btn-small" onClick={() => setShowSettings(false)}
              aria-label="Close settings">
              <X size={16} /> Close
            </button>
          </header>

          {/* ── Provider API Keys ── */}
          <section>
            <div className="settings-section-header">
              <h3>AI Providers</h3>
            </div>
            <div className="card">
              <div className="form-group">
                <label>OpenAI API Key</label>
                <div className="api-key-row">
                  <input className="plaud-input api-key-input" type="password"
                    placeholder="sk-..." value={openaiKey}
                    onChange={e => handleOpenaiKeyChange(e.target.value)}
                    style={{ flex: 1 }} aria-label="OpenAI API Key" />
                  {openaiKey && <span className="pill pill-green">Saved</span>}
                </div>
              </div>
              <div className="form-group">
                <label>Anthropic API Key</label>
                <div className="api-key-row">
                  <input className="plaud-input api-key-input" type="password"
                    placeholder="sk-ant-..." value={anthropicKey}
                    onChange={e => handleAnthropicKeyChange(e.target.value)}
                    style={{ flex: 1 }} aria-label="Anthropic API Key" />
                  {anthropicKey && <span className="pill pill-green">Saved</span>}
                </div>
              </div>
            </div>
          </section>

          {/* ── Model Preferences ── */}
          <section>
            <div className="settings-section-header">
              <h3>Default Model</h3>
            </div>
            <div className="card">
              <div className="form-group">
                <label>AI Provider</label>
                <div className="select-wrapper" data-tooltip="Choose which AI backend to use for summarization and analysis">
                  <select className="plaud-select" value={provider}
                    onChange={e => handleProviderChange(e.target.value)} aria-label="AI Provider">
                    <option value="ollama">Ollama (Local)</option>
                    <option value="openai" disabled={!openaiKey}>OpenAI {!openaiKey && '(key not set)'}</option>
                    <option value="anthropic" disabled={!anthropicKey}>Anthropic {!anthropicKey && '(key not set)'}</option>
                  </select>
                  <ChevronDown className="select-icon" size={16} />
                </div>
              </div>
              <div className="form-group">
                <label>Model</label>
                <div className="select-wrapper">
                  <select className="plaud-select" value={selectedLlm}
                    onChange={e => setSelectedLlm(e.target.value)} aria-label="AI Model">
                    {getModelOptions().length === 0 && <option value="">No models available</option>}
                    {getModelOptions().map(m => (
                      <option key={m.name} value={m.name}>{m.name} {m.recommended ? '⭐' : ''}</option>
                    ))}
                  </select>
                  <ChevronDown className="select-icon" size={16} />
                </div>
              </div>
            </div>
          </section>

          {/* ── Audio Recording ── */}
          <section>
            <div className="settings-section-header">
              <h3>Audio Recording</h3>
            </div>
            <div className="card">
              <div className="form-group">
                <label>Microphone</label>
                <div className="select-wrapper">
                  <select className="plaud-select" value={selectedAudioDeviceId}
                    onChange={e => { setSelectedAudioDeviceId(e.target.value); localStorage.setItem("postilla_audio_device", e.target.value); }}
                    aria-label="Microphone">
                    <option value="default">Default Microphone</option>
                    {audioDevices.map(d => (
                      <option key={d.deviceId} value={d.deviceId}>{d.label || `Device (${d.deviceId.slice(0, 8)}...)`}</option>
                    ))}
                  </select>
                  <ChevronDown className="select-icon" size={16} />
                </div>
                <button className="plaud-btn btn-ghost btn-small" onClick={loadAudioDevices}
                  style={{ marginTop: '0.25rem', alignSelf: 'flex-start' }} aria-label="Refresh audio devices">
                  <RefreshCw size={12} /> Refresh devices
                </button>
              </div>
              <div className="form-group">
                <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                  <input type="checkbox" checked={captureSystemAudio}
                    onChange={e => { setCaptureSystemAudio(e.target.checked); localStorage.setItem("postilla_capture_system_audio", String(e.target.checked)); }}
                    aria-label="Capture system audio" />
                  Capture system audio (for Teams calls, etc.)
                </label>
                <p style={{ fontSize: '0.75rem', color: '#8e8ea0', margin: '0.25rem 0 0 0' }}>
                  When enabled, both your microphone and system audio will be mixed into one recording.
                </p>
              </div>
              {captureSystemAudio && (
                <div className="form-group">
                  <label>System Audio Device</label>
                  <div className="select-wrapper">
                    <select className="plaud-select" value={systemAudioDeviceId}
                      onChange={e => { setSystemAudioDeviceId(e.target.value); localStorage.setItem("postilla_system_audio_device", e.target.value); }}
                      aria-label="System audio device">
                      <option value="">-- Select device --</option>
                      {audioDevices.map(d => (
                        <option key={d.deviceId} value={d.deviceId}>{d.label || `Device (${d.deviceId.slice(0, 8)}...)`}</option>
                      ))}
                    </select>
                    <ChevronDown className="select-icon" size={16} />
                  </div>
                  <p style={{ fontSize: '0.75rem', color: '#8e8ea0', margin: '0.25rem 0 0 0' }}>
                    On Linux, select a "Monitor of ..." source to capture system audio.
                  </p>
                </div>
              )}
            </div>
          </section>

          {/* ── Summarization Templates ── */}
          <section>
            <div className="settings-section-header">
              <h3>Summarization Templates</h3>
              <button className="plaud-btn btn-primary btn-small" onClick={startNewTemplate}>
                + New Template
              </button>
            </div>

            {editingTemplate !== undefined && (
              <div className="card template-editor">
                <h4>{editingTemplate ? `Edit: ${editingTemplate.name}` : "New Template"}</h4>
                <div className="form-group">
                  <label>Name</label>
                  <input className="plaud-input" value={editName} onChange={e => setEditName(e.target.value)} placeholder="Template name" />
                </div>
                <div className="form-group">
                  <label>Session Type</label>
                  <div className="select-wrapper">
                    <select className="plaud-select" value={editType} onChange={e => setEditType(e.target.value)}>
                      {SESSION_TYPE_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
                    </select>
                    <ChevronDown className="select-icon" size={16} />
                  </div>
                </div>
                <div className="form-group">
                  <label>System Prompt</label>
                  <textarea className="plaud-textarea" value={editPrompt}
                    onChange={e => setEditPrompt(e.target.value)}
                    placeholder="Write the system prompt for the LLM..."
                    rows={8} />
                </div>
                <div className="form-actions">
                  <button className="plaud-btn btn-primary" onClick={handleSaveTemplate}>Save</button>
                  <button className="plaud-btn btn-outline" onClick={cancelTemplateEdit}>Cancel</button>
                </div>
              </div>
            )}

            <div className="template-list">
              {templates.map(tpl => (
                <div key={tpl.id} className="card template-card">
                  <div className="template-card-header">
                    <strong>{tpl.name}</strong>
                    <span className="pill">{SESSION_TYPE_LABELS[tpl.session_type] || tpl.session_type}</span>
                  </div>
                  <p className="template-preview">{tpl.system_prompt.slice(0, 100)}...</p>
                  <div className="template-actions">
                    <button className="plaud-btn btn-outline btn-small" onClick={() => startTemplateEdit(tpl)}>
                      <Pencil size={14} /> Edit
                    </button>
                    <button className="plaud-btn btn-danger btn-small" onClick={() => handleDeleteTemplate(tpl.id)}>
                      Delete
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </section>

          {/* ── Export Templates ── */}
          <section>
            <div className="settings-section-header">
              <h3>Export Templates</h3>
              <button className="plaud-btn btn-primary btn-small" onClick={startNewExportTemplate}>
                + New Template
              </button>
            </div>

            <div className="export-template-list">
              {exportTemplates.map(tpl => (
                <div key={tpl.id} className="card template-card">
                  <div className="template-card-header">
                    <strong>{tpl.name}</strong>
                  </div>
                  <p className="template-preview">{tpl.body.slice(0, 100)}...</p>
                  <div className="template-actions">
                    <button className="plaud-btn btn-outline btn-small" onClick={() => editExportTemplate(tpl)}>
                      <Pencil size={14} /> Edit
                    </button>
                    <button className="plaud-btn btn-danger btn-small" onClick={() => deleteExportTemplateFn(tpl.id)}>
                      Delete
                    </button>
                  </div>
                </div>
              ))}
            </div>

            {editingExportTemplate !== undefined && (
              <div className="card template-editor">
                <h4>{editingExportTemplate ? `Edit: ${editingExportTemplate.name}` : "New Export Template"}</h4>
                <div className="form-group">
                  <label>Name</label>
                  <input className="plaud-input" value={editExportName} onChange={e => setEditExportName(e.target.value)} placeholder="Template name" />
                </div>
                <div className="form-group">
                  <label>Body template</label>
                  <textarea className="plaud-textarea" rows={10}
                    value={editExportBody}
                    onChange={e => setEditExportBody(e.target.value)}
                    placeholder="Use {title}, {type}, {date}, {participants}, {tags}, {transcript}, {summary}, {mind_map}" />
                </div>
                <p style={{ fontSize: '0.8rem', color: '#8e8ea0' }}>
                  Available placeholders: {`{title}`}, {`{type}`}, {`{date}`}, {`{participants}`}, {`{tags}`}, {`{transcript}`}, {`{summary}`}, {`{mind_map}`}
                </p>
                <div className="template-actions" style={{ marginTop: '0.5rem' }}>
                  <button className="plaud-btn btn-primary btn-small" onClick={saveExportTemplateFn}>Save</button>
                  <button className="plaud-btn btn-outline btn-small" onClick={() => { setEditingExportTemplate(null); setEditExportName(""); setEditExportBody(""); }}>Cancel</button>
                </div>
              </div>
            )}
          </section>

          {/* ── Backup & Restore ── */}
          <section>
            <div className="settings-section-header">
              <h3>Data</h3>
            </div>
            <div className="card">
              <div className="form-group">
                <p style={{ fontSize: '0.85rem', color: '#8e8ea0', margin: 0 }}>Backup or restore your entire database (sessions, templates, settings).</p>
                <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.75rem' }}>
                  <button className="plaud-btn btn-primary btn-small" onClick={handleBackup}>
                    <Download size={14} /> Backup DB
                  </button>
                  <button className="plaud-btn btn-outline btn-small" onClick={handleRestore}>
                    <RefreshCw size={14} /> Restore DB
                  </button>
                </div>
              </div>
            </div>
          </section>

          {/* ── Auto-Delete ── */}
          <section>
            <div className="settings-section-header">
              <h3>Auto-Delete</h3>
            </div>
            <div className="card">
              <div className="form-group">
                <p style={{ fontSize: '0.85rem', color: '#8e8ea0', margin: 0 }}>Automatically delete sessions older than the specified number of days.</p>
                <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center', marginTop: '0.75rem' }}>
                  <input className="plaud-input" type="number" id="autoDeleteDays" defaultValue={90}
                    style={{ width: 80 }} min={1} />
                  <span style={{ fontSize: '0.85rem', color: '#8e8ea0' }}>days</span>
                  <button className="plaud-btn btn-danger btn-small" onClick={async () => {
                    const days = parseInt((document.getElementById('autoDeleteDays') as HTMLInputElement).value);
                    if (!days || days < 1) return;
                    if (!confirm(`Delete all sessions older than ${days} days? This cannot be undone.`)) return;
                    try {
                      const count = await invoke<number>("cleanup_old_sessions", { days });
                      alert(`Deleted ${count} old session(s).`);
                      await loadSessions();
                    } catch (err: any) { alert("Auto-delete failed: " + (err.message || err)); }
                  }}>Clean Up</button>
                </div>
              </div>
            </div>
          </section>

          {/* ── Dashboard ── */}
          <section id="dashboard-section">
            <div className="settings-section-header">
              <h3>Dashboard</h3>
            </div>
            <div className="card">
              {(() => {
                const [stats, setStats] = useState<Record<string, any> | null>(null);
                useEffect(() => {
                  invoke<Record<string, any>>("get_dashboard_stats").then(setStats).catch(() => {});
                }, []);
                if (!stats) return <p style={{ fontSize: '0.85rem', color: '#8e8ea0' }}>Loading stats...</p>;
                return (
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '0.75rem' }}>
                    <div className="stat-box"><strong>{stats.total}</strong><span>Total Sessions</span></div>
                    <div className="stat-box"><strong>{stats.with_transcript}</strong><span>Transcribed</span></div>
                    <div className="stat-box"><strong>{stats.with_summary}</strong><span>Summarized</span></div>
                    <div className="stat-box"><strong>{stats.total_audio_minutes}</strong><span>Audio Minutes</span></div>
                    <div className="stat-box"><strong>{stats.by_type?.meeting || 0}</strong><span>Meetings</span></div>
                    <div className="stat-box"><strong>{stats.by_type?.voice_note || 0}</strong><span>Voice Notes</span></div>
                    <div className="stat-box"><strong>{stats.by_type?.lecture || 0}</strong><span>Lectures</span></div>
                    <div className="stat-box"><strong>{stats.by_type?.import || 0}</strong><span>Imported</span></div>
                  </div>
                );
              })()}
            </div>
          </section>
        </div>
      </div>
    );
  }

  // ──────── MAIN VIEW ────────
  return (
    <div className="app-layout">
      {showOnboarding && (
        <div className="onboarding-overlay">
          <div className="onboarding-card">
            <div className="onboarding-logo"><Sparkles size={40} color="#8b5cf6" /></div>
            <h2>Welcome to Postilla</h2>
            <p>Your local-first AI assistant for meetings, voice notes, lectures, and more.</p>
            <div className="onboarding-steps">
              <div className="onboarding-step">
                <div className="step-number">1</div>
                <div><strong>Record or Import</strong> — Capture audio directly or import an existing file.</div>
              </div>
              <div className="onboarding-step">
                <div className="step-number">2</div>
                <div><strong>Transcribe</strong> — Automatically convert speech to text with local AI.</div>
              </div>
              <div className="onboarding-step">
                <div className="step-number">3</div>
                <div><strong>Summarize</strong> — Get AI-powered summaries and mind maps from your transcripts.</div>
              </div>
              <div className="onboarding-step">
                <div className="step-number">4</div>
                <div><strong>AI Providers</strong> — Configure Ollama (local), OpenAI, or Anthropic in Settings.</div>
              </div>
              <div className="onboarding-step">
                <div className="step-number">5</div>
                <div><strong>Export</strong> — Save as Markdown, TXT, SRT, VTT, PDF, or Obsidian.</div>
              </div>
            </div>
            <button className="plaud-btn btn-primary" style={{ width: '100%', marginTop: '1rem' }}
              onClick={() => {
                localStorage.setItem("postilla_onboarding_done", "1");
                setShowOnboarding(false);
              }} aria-label="Get started — close onboarding">
              Get Started
            </button>
          </div>
        </div>
      )}
      <main className="main-content">
        <nav className="breadcrumb" aria-label="Breadcrumb">
          <span className="breadcrumb-item active" onClick={() => { setSelectedSessionId(null); setCompareMode(false); }}
            role="link" tabIndex={0} aria-label="Home">Home</span>
          {compareMode && (
            <>
              <span className="breadcrumb-sep">›</span>
              <span className="breadcrumb-item active">Compare</span>
            </>
          )}
          {selectedSession && !compareMode && (
            <>
              <span className="breadcrumb-sep">›</span>
              <span className="breadcrumb-item" onClick={() => setSelectedSessionId(null)}
                role="link" tabIndex={0} aria-label="Back to sessions">
                {SESSION_TYPE_LABELS[selectedSession.session_type] || selectedSession.session_type}
              </span>
              <span className="breadcrumb-sep">›</span>
              <span className="breadcrumb-item active">{selectedSession.title}</span>
            </>
          )}
        </nav>
        {compareMode ? (
          <div className="compare-view">
            <header className="compare-header">
              <h2>Compare Sessions</h2>
              <button className="plaud-btn btn-outline btn-small" onClick={() => setCompareMode(false)}
                aria-label="Close compare view">
                <X size={16} /> Close
              </button>
            </header>
            <div className="compare-columns">
              {[0, 1].map(idx => (
                <div key={idx} className="compare-column">
                  <div className="select-wrapper" style={{ marginBottom: '1rem' }}>
                    <select className="plaud-select"
                      value={compareIds[idx] ?? ''}
                      onChange={e => {
                        const newIds = [...compareIds] as [number | null, number | null];
                        newIds[idx] = e.target.value ? Number(e.target.value) : null;
                        setCompareIds(newIds);
                      }}>
                      <option value="">-- Select session {idx + 1} --</option>
                      {sessions.filter(s => s.id !== compareIds[1 - idx]).map(s => (
                        <option key={s.id} value={s.id}>{s.title}</option>
                      ))}
                    </select>
                  </div>
                  {compareIds[idx] && (() => {
                    const s = sessions.find(s => s.id === compareIds[idx]);
                    if (!s) return null;
                    return (
                      <div className="compare-session">
                        <h3>{s.title}</h3>
                        <div className="compare-meta">
                          <span className="pill">{SESSION_TYPE_LABELS[s.session_type] || s.session_type}</span>
                          <span>{new Date(s.created_at).toLocaleDateString()}</span>
                        </div>
                        {s.tags && (
                          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', margin: '0.5rem 0' }}>
                            {s.tags.split(',').map(t => t.trim()).filter(Boolean).map(t => (
                              <span key={t} className="tag-badge" style={{ fontSize: '0.65rem', padding: '1px 6px', borderRadius: '8px', background: 'rgba(139,92,246,0.2)', color: '#8b5cf6' }}>{t}</span>
                            ))}
                          </div>
                        )}
                        {s.transcript && (
                          <div className="compare-section">
                            <h4>Transcript</h4>
                            <div className="compare-content">{renderAnnotated(s.transcript)}</div>
                          </div>
                        )}
                        {s.summary && (
                          <div className="compare-section">
                            <h4>Summary</h4>
                            <div className="compare-content">{s.summary}</div>
                          </div>
                        )}
                      </div>
                    );
                  })()}
                </div>
              ))}
            </div>
          </div>
        ) : selectedSession ? (
          <div className="detail-view">
            {/* ── Header ── */}
            <header className="detail-header">
              <div className="title-row">
                <input
                  className="title-input"
                  value={selectedSession.title}
                  onChange={e => handleUpdateTitle(selectedSession.id, e.target.value)}
                  placeholder="Session title..."
                />
                {titlingId === selectedSession.id && <RefreshCw size={16} className="spin" />}
              </div>
              <div className="detail-meta">
                <div className="select-wrapper" style={{ minWidth: 140 }}>
                  <select className="plaud-select"
                    value={selectedSession.session_type}
                    onChange={e => handleChangeSessionType(selectedSession.id, e.target.value)}
                    aria-label="Session type" data-tooltip="Change the session type">
                    {SESSION_TYPE_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
                  </select>
                  <ChevronDown className="select-icon" size={16} />
                </div>
                {durations[selectedSession.id] && <span className="duration">{durations[selectedSession.id]}</span>}
                <span className="date">
                  {new Date(selectedSession.created_at).toLocaleDateString(undefined, {
                    weekday: 'long', year: 'numeric', month: 'long', day: 'numeric',
                    hour: '2-digit', minute: '2-digit'
                  })}
                </span>
                <div className="export-actions" role="group" aria-label="Export actions">
                  <button className="plaud-btn btn-outline btn-small"
                    onClick={() => handleExportMarkdown(selectedSession.id, selectedSession.title)}
                    aria-label="Export as Markdown" data-tooltip="Export as Markdown">
                    .md
                  </button>
                  <button className="plaud-btn btn-outline btn-small"
                    onClick={() => handleExportTxt(selectedSession.id, selectedSession.title)}
                    aria-label="Export as TXT" data-tooltip="Export as plain text">
                    TXT
                  </button>
                  <button className="plaud-btn btn-outline btn-small"
                    onClick={() => handleExportSrt(selectedSession.id, selectedSession.title)}
                    aria-label="Export as SRT subtitles" data-tooltip="Export as SRT subtitles">
                    SRT
                  </button>
                  <button className="plaud-btn btn-outline btn-small"
                    onClick={() => handleExportVtt(selectedSession.id, selectedSession.title)}
                    aria-label="Export as VTT subtitles" data-tooltip="Export as WebVTT subtitles">
                    VTT
                  </button>
                  <button className="plaud-btn btn-outline btn-small"
                    onClick={() => handleExportObsidian(selectedSession.id, selectedSession.title)}
                    aria-label="Export for Obsidian" data-tooltip="Export as Obsidian Markdown with frontmatter">
                    Obs
                  </button>
                  <button className="plaud-btn btn-outline btn-small"
                    onClick={() => handleExportPdf(selectedSession.id, selectedSession.title)}
                    aria-label="Export as PDF" data-tooltip="Open print dialog for PDF">
                    PDF
                  </button>
                  {exportTemplates.length > 0 && (
                    <div className="select-wrapper" style={{ minWidth: 100 }}>
                      <select className="plaud-select" style={{ fontSize: '0.75rem', padding: '2px 20px 2px 6px' }}
                        value="" onChange={e => { if (e.target.value) handleExportWithTemplate(selectedSession.id, Number(e.target.value)); }}
                        aria-label="Export with template">
                        <option value="">Template...</option>
                        {exportTemplates.map(t => (
                          <option key={t.id} value={t.id}>{t.name}</option>
                        ))}
                      </select>
                    </div>
                  )}
                  <button className="plaud-btn btn-danger btn-small"
                    onClick={() => handleDeleteSession(selectedSession.id)}
                    aria-label="Delete session" data-tooltip="Permanently delete this session">
                    <X size={14} />
                  </button>
                </div>
              </div>
            </header>

            <div className="detail-body">
              {/* ── Audio Section ── */}
              {!selectedSession.file_path && (
                <div className="card action-card">
                  <div className="action-card-content">
                    <div className="action-circle"><Mic size={32} color="#007aff" /></div>
                    <h3>Ready to capture</h3>
                    <p>Start recording your voice or import an existing audio file.</p>
                    {recordingId !== selectedSession.id && (
                      <div className="audio-source-selector">
                        <div className="select-wrapper" style={{ minWidth: 220 }}>
                          <select className="plaud-select"
                            value={selectedAudioDeviceId}
                            onChange={e => { setSelectedAudioDeviceId(e.target.value); localStorage.setItem("postilla_audio_device", e.target.value); }}
                            aria-label="Audio source">
                            <option value="default">Default Microphone</option>
                            {audioDevices.map(d => (
                              <option key={d.deviceId} value={d.deviceId}>{d.label || `Device (${d.deviceId.slice(0, 8)}...)`}</option>
                            ))}
                          </select>
                          <ChevronDown className="select-icon" size={16} />
                        </div>
                        {captureSystemAudio && systemAudioDeviceId && systemAudioDeviceId !== selectedAudioDeviceId && (
                          <span className="pill" style={{ fontSize: '0.65rem' }}>System Audio On</span>
                        )}
                        {audioDevices.length === 0 && (
                          <button className="plaud-btn btn-ghost btn-small" onClick={loadAudioDevices}
                            aria-label="Refresh audio devices">
                            <RefreshCw size={14} /> Refresh
                          </button>
                        )}
                      </div>
                    )}
                    <div className="actions-center">
                      {recordingId === selectedSession.id ? (
                        <button className="plaud-btn btn-danger active" onClick={stopRecording}
                          aria-label="Stop recording" data-tooltip="Stop the current recording">
                          <MicOff size={18} /> Stop Recording
                        </button>
                      ) : (
                        <button className="plaud-btn btn-primary" onClick={() => startRecording(selectedSession.id)} disabled={recordingId !== null}
                          aria-label="Record audio" data-tooltip="Start recording from your microphone">
                          <Mic size={18} /> Record Audio
                        </button>
                      )}
                      <button className="plaud-btn btn-outline" onClick={() => handleImport(selectedSession.id)} disabled={recordingId !== null}
                        aria-label="Import audio file" data-tooltip="Import an existing audio file from your computer">
                        <FileAudio size={18} /> Import File
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {selectedSession.file_path && (
                <div className="card media-card">
                  <div className="media-info">
                    <div className="media-icon"><Play size={20} color="#1d1d1f" /></div>
                    <div className="media-details">
                      <strong>Audio File</strong>
                      <span>{selectedSession.file_path.split('/').pop() || selectedSession.file_path.split('\\').pop()}</span>
                    </div>
                  </div>

                  {/* Play button */}
                  <AudioPlayer filePath={selectedSession.file_path} />

                  {/* Participants input */}
                  {!selectedSession.transcript && (
                      <div className="participants-section" style={{ marginTop: '0.75rem' }}>
                      <div className="participants-row">
                        <input className="plaud-input"
                          placeholder="Participants (e.g. Francesco, Giovanni)"
                          value={selectedSession.participants || ''}
                          onChange={e => handleUpdateParticipants(selectedSession.id, e.target.value)}
                          aria-label="Session participants" data-tooltip="List participants separated by commas"
                        />
                      </div>
                    </div>
                  )}

                  {/* Tags input */}
                  <div className="tags-section" style={{ marginTop: '0.5rem' }}>
                    <div className="tags-row">
                      <input className="plaud-input"
                        placeholder="Tags (comma separated, e.g. design, sprint, urgent)"
                        value={selectedSession.tags || ''}
                        onChange={e => handleUpdateTags(selectedSession.id, e.target.value)}
                        aria-label="Session tags" data-tooltip="Add tags separated by commas for easy filtering"
                      />
                    </div>
                  </div>

                  {/* Transcribe action */}
                  {!selectedSession.transcript && (
                    <div className="transcribe-action" style={{ marginTop: '0.75rem' }}>
                      <div className="select-wrapper">
                        <Languages className="select-icon-left" size={16} />
                        <select className="plaud-select lang-select" value={transcribeLang} onChange={e => setTranscribeLang(e.target.value)}
                          aria-label="Transcription language" data-tooltip="Select the language of the audio for better accuracy">
                          <option value="auto">Auto-detect Language</option>
                          <option value="it">Italian (it)</option>
                          <option value="en">English (en)</option>
                          <option value="fr">French (fr)</option>
                          <option value="es">Spanish (es)</option>
                          <option value="de">German (de)</option>
                        </select>
                        <ChevronDown className="select-icon" size={16} />
                      </div>
                      <button className="plaud-btn btn-accent"
                        onClick={() => runAutoPipeline(selectedSession.id)}
                        disabled={transcribingId !== null || summarizingId !== null || mindMappingId !== null}
                        aria-label="Run AI pipeline" data-tooltip="Automatically transcribe, summarize, and generate mind map">
                        {transcribingId === selectedSession.id
                          ? <><RefreshCw size={18} className="spin" /> Transcribing...</>
                          : summarizingId === selectedSession.id
                          ? <><RefreshCw size={18} className="spin" /> Summarizing...</>
                          : mindMappingId === selectedSession.id
                          ? <><RefreshCw size={18} className="spin" /> Mind Map...</>
                          : <><Sparkles size={18} /> AI Pipeline</>}
                      </button>
                    </div>
                  )}

                  {transcribingId === selectedSession.id && downloadProgress && (
                    <div className="progress-container">
                      <div className="progress-text">{downloadProgress.item} ({Math.round(downloadProgress.progress)}%)</div>
                      <div className="progress-bar"><div className="progress-fill" style={{ width: `${downloadProgress.progress}%` }}></div></div>
                    </div>
                  )}
                </div>
              )}

              {/* ── Transcript Card ── */}
              {selectedSession.transcript && (
                <div className="card transcript-card">
                  <div className="card-header">
                    <h3><span className="collapse-toggle" onClick={() => toggleSection('transcript')}>
                      <ChevronDown size={14} className={`collapse-chevron ${collapsedSections.has('transcript') ? '' : 'open'}`} />
                    </span> Transcript</h3>
                    <div className="card-header-actions">
                      <button className="plaud-btn btn-outline btn-small"
                        onClick={() => startEditing(selectedSession.id, 'transcript')}
                        disabled={editingFields[selectedSession.id]?.transcript}
                        aria-label="Edit transcript" data-tooltip="Edit transcript manually">
                        <Pencil size={14} /> Edit
                      </button>
                      <button className="plaud-btn btn-outline btn-small"
                        onClick={() => handleTranscribe(selectedSession.id)}
                        disabled={transcribingId !== null}
                        aria-label="Re-transcribe" data-tooltip="Re-run transcription">
                        {transcribingId === selectedSession.id
                          ? <><RefreshCw size={14} className="spin" /> Transcribing...</>
                          : <><RefreshCw size={14} /> Re-transcribe</>}
                      </button>
                    </div>
                  </div>
                  {!collapsedSections.has('transcript') && (
                    <>
                  {editingFields[selectedSession.id]?.transcript ? (
                    <textarea className="inline-editor"
                      defaultValue={selectedSession.transcript}
                      autoFocus aria-label="Transcript editor"
                      onBlur={e => saveTranscript(selectedSession.id, e.target.value)}
                      onKeyDown={e => {
                        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                          saveTranscript(selectedSession.id, (e.target as HTMLTextAreaElement).value);
                        }
                        if (e.key === 'Escape') {
                          stopEditing(selectedSession.id, 'transcript');
                        }
                      }}
                    />
                  ) : (
                    <div className="transcript-text" onDoubleClick={() => startEditing(selectedSession.id, 'transcript')}>
                      {renderAnnotated(selectedSession.transcript)}
                    </div>
                  )}
                  {selectedSession.participants && selectedSession.participants.trim() && (
                    <div className="annotate-action" style={{ marginTop: '0.75rem' }}>
                      <button className="plaud-btn btn-outline btn-small"
                        onClick={() => handleAnnotateSpeakers(selectedSession.id)}
                        disabled={annotatingId !== null || !selectedLlm}
                        aria-label="Annotate speakers" data-tooltip="Label speaker names in the transcript">
                        {annotatingId === selectedSession.id
                          ? <><RefreshCw size={14} className="spin" /> Annotating Speakers...</>
                          : <><Users size={14} /> Annotate Speakers</>}
                      </button>
                    </div>
                  )}
                    </>
                  )}
                </div>
              )}

              {/* ── Summary Card ── */}
              {selectedSession.transcript && !selectedSession.summary && (
                <div className="card summary-card">
                  <div className="card-header">
                    <h3><Sparkles size={18} /> AI Summary</h3>
                    <div className="llm-action">
                      {!ollamaConnected && provider === "ollama" ? (
                        <span className="error-text text-small">Ollama not running</span>
                      ) : (
                        <>
                          {/* Provider selector */}
                          <div className="select-wrapper">
                            <select className="plaud-select llm-select"
                              value={provider} onChange={e => handleProviderChange(e.target.value)}
                              aria-label="AI Provider for summarization">
                              <option value="ollama">Ollama (Local)</option>
                              {openaiKey && <option value="openai">OpenAI</option>}
                              {anthropicKey && <option value="anthropic">Anthropic</option>}
                            </select>
                            <ChevronDown className="select-icon" size={16} />
                          </div>
                          {provider === "openai" && (
                            <input className="plaud-input api-key-input" type="password"
                              placeholder="sk-..." value={openaiKey}
                              onChange={e => handleOpenaiKeyChange(e.target.value)}
                              aria-label="OpenAI API Key" />
                          )}
                          {provider === "anthropic" && (
                            <input className="plaud-input api-key-input" type="password"
                              placeholder="sk-ant-..." value={anthropicKey}
                              onChange={e => handleAnthropicKeyChange(e.target.value)}
                              aria-label="Anthropic API Key" />
                          )}
                          {/* Template selector */}
                          <div className="select-wrapper">
                            <Settings2 className="select-icon-left" size={16} />
                            <select className="plaud-select llm-select"
                              value={selectedTemplateId ?? ''}
                              onChange={e => setSelectedTemplateId(e.target.value ? Number(e.target.value) : null)}>
                              {templates.filter(t => t.session_type === selectedSession.session_type).map(t => (
                                <option key={t.id} value={t.id}>{t.name}</option>
                              ))}
                              {templates.filter(t => t.session_type !== selectedSession.session_type).map(t => (
                                <option key={t.id} value={t.id}>{t.name} ({SESSION_TYPE_LABELS[t.session_type]})</option>
                              ))}
                            </select>
                            <ChevronDown className="select-icon" size={16} />
                          </div>
                          {/* LLM selector */}
                          <div className="select-wrapper">
                            <select className="plaud-select llm-select"
                              value={selectedLlm} onChange={e => setSelectedLlm(e.target.value)}>
                              {getModelOptions().map(m => (
                                <option key={m.name} value={m.name}>{m.name} {m.recommended ? '⭐' : ''}</option>
                              ))}
                            </select>
                            <ChevronDown className="select-icon" size={16} />
                          </div>
                          <button className="plaud-btn btn-primary btn-small"
                            onClick={() => handleSummarize(selectedSession.id)}
                            disabled={summarizingId !== null || !selectedLlm}>
                            {summarizingId === selectedSession.id
                              ? <><RefreshCw size={14} className="spin" /> Summarizing...</>
                              : <><Sparkles size={14} /> Summarize</>}
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              )}

              {selectedSession.summary && (
                <div className="card summary-card">
                  <div className="card-header">
                    <h3 className="gradient-text"><span className="collapse-toggle" onClick={() => toggleSection('summary')}>
                      <ChevronDown size={14} className={"collapse-chevron " + (collapsedSections.has('summary') ? '' : 'open')} />
                    </span><Sparkles size={18} /> AI Summary</h3>
                    <div className="llm-action">
                      <button className="plaud-btn btn-outline btn-small"
                        onClick={() => startEditing(selectedSession.id, 'summary')}
                        disabled={editingFields[selectedSession.id]?.summary}>
                        <Pencil size={14} /> Edit
                      </button>
                      <div className="select-wrapper">
                        <select className="plaud-select llm-select"
                          value={provider} onChange={e => handleProviderChange(e.target.value)}>
                          <option value="ollama">Ollama (Local)</option>
                          {openaiKey && <option value="openai">OpenAI</option>}
                          {anthropicKey && <option value="anthropic">Anthropic</option>}
                        </select>
                        <ChevronDown className="select-icon" size={16} />
                      </div>
                      <div className="select-wrapper">
                        <select className="plaud-select llm-select"
                          value={selectedLlm} onChange={e => setSelectedLlm(e.target.value)}>
                          {getModelOptions().map(m => (
                            <option key={m.name} value={m.name}>{m.name} {m.recommended ? '⭐' : ''}</option>
                          ))}
                        </select>
                        <ChevronDown className="select-icon" size={16} />
                      </div>
                      <button className="plaud-btn btn-outline btn-small"
                        onClick={() => handleSummarize(selectedSession.id)}
                        disabled={summarizingId !== null || !selectedLlm}>
                        {summarizingId === selectedSession.id
                          ? <><RefreshCw size={14} className="spin" /> Summarizing...</>
                          : <><RefreshCw size={14} /> Retry</>}
                      </button>
                    </div>
                  </div>
                  {!collapsedSections.has('summary') && (
                  <>
                  {editingFields[selectedSession.id]?.summary ? (
                    <textarea className="inline-editor"
                      defaultValue={selectedSession.summary}
                      autoFocus
                      onBlur={e => saveSummary(selectedSession.id, e.target.value)}
                      onKeyDown={e => {
                        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                          saveSummary(selectedSession.id, (e.target as HTMLTextAreaElement).value);
                        }
                        if (e.key === 'Escape') {
                          stopEditing(selectedSession.id, 'summary');
                        }
                      }}
                    />
                  ) : (
                    <div className="summary-text" onDoubleClick={() => startEditing(selectedSession.id, 'summary')}>
                      {selectedSession.summary.split('\n').map((line, i) => (
                        <span key={i}>{line}<br /></span>
                      ))}
                    </div>
                  )}
                  </>
                  )}
                </div>
              )}

              {/* ── Mind Map Card ── */}
              {(selectedSession.transcript || selectedSession.summary) && (
                <div className="card mindmap-card">
                  <div className="card-header">
                    <h3><span className="collapse-toggle" onClick={() => toggleSection('mindmap')}>
                      <ChevronDown size={14} className={"collapse-chevron " + (collapsedSections.has('mindmap') ? '' : 'open')} />
                    </span><TreePine size={18} /> Mind Map</h3>
                    {!selectedSession.mind_map && (
                      <button className="plaud-btn btn-accent btn-small"
                        onClick={() => handleGenerateMindMap(selectedSession.id)}
                        disabled={mindMappingId !== null || !selectedLlm}>
                        {mindMappingId === selectedSession.id
                          ? <><RefreshCw size={14} className="spin" /> Generating...</>
                          : <><BrainCircuit size={14} /> Generate</>}
                      </button>
                    )}
                  {!collapsedSections.has('mindmap') && selectedSession.mind_map && (
                      <button className="plaud-btn btn-outline btn-small"
                        onClick={() => handleGenerateMindMap(selectedSession.id)}
                        disabled={mindMappingId !== null || !selectedLlm}>
                        {mindMappingId === selectedSession.id
                          ? <><RefreshCw size={14} className="spin" /> Generating...</>
                          : <><RefreshCw size={14} /> Regenerate</>}
                      </button>
                    )}
                  </div>
                  {selectedSession.mind_map && (
                    <div className="mindmap-content">
                      {selectedSession.mind_map.split('\n').map((line, i) => {
                        if (line.startsWith('### ')) return <div key={i} className="mm-node mm-level-3">{line.replace('### ', '').trim()}</div>;
                        if (line.startsWith('## ')) return <div key={i} className="mm-node mm-level-2">{line.replace('## ', '').trim()}</div>;
                        if (line.startsWith('# ')) return <div key={i} className="mm-node mm-level-1">{line.replace('# ', '').trim()}</div>;
                        if (line.trim()) return <div key={i} className="mm-node mm-text">{line}</div>;
                        return null;
                      })}
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>
        ) : (
          /* ── Empty State with Quick Actions ── */
          <div className="empty-state">
            <div className="empty-state-icon"><Mic size={48} color="#007aff" strokeWidth={1.5} /></div>
            <h2>Capture your thoughts</h2>
            <p>Choose a session type to get started.</p>

            <div className="quick-create-grid">
              {SESSION_TYPE_OPTIONS.map(opt => (
                <button key={opt.value} className="card quick-create-card"
                  onClick={() => quickCreateSession(opt.value)}
                  aria-label={`Create new ${opt.label}`}>
                  <div className="quick-create-icon">
                    {opt.value === 'meeting' && <Mic size={24} />}
                    {opt.value === 'voice_note' && <Sparkles size={24} />}
                    {opt.value === 'lecture' && <FileAudio size={24} />}
                    {opt.value === 'import' && <FileAudio size={24} />}
                  </div>
                  <span>{opt.label}</span>
                </button>
              ))}
            </div>
          </div>
        )}
      </main>

      {/* ── Sidebar ── */}
      <aside className="sidebar-left">
        <div className="sidebar-brand">
          <div className="sidebar-brand-icon">
            <Sparkles size={16} />
          </div>
          <span>Postilla</span>
        </div>

        <div className="sidebar-actions" role="toolbar" aria-label="Main actions">
          <button className="sidebar-btn sidebar-btn-primary" onClick={() => setSelectedSessionId(null)}
            aria-label="New session" data-tooltip="Create a new session">
            <Mic size={14} /> New
          </button>
          <button className={`sidebar-btn ${compareMode ? 'active' : ''}`} onClick={() => { setCompareMode(!compareMode); if (!compareMode) setSelectedSessionId(null); }}
            aria-label="Compare sessions" data-tooltip="Compare two sessions side by side">
            <BrainCircuit size={14} />
          </button>
          <button className="sidebar-btn" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
            aria-label="Toggle theme" data-tooltip="Switch between light and dark mode">
            {theme === 'dark' ? <Sparkles size={14} /> : <Sparkles size={14} />}
          </button>
          <button className="sidebar-btn" onClick={() => {
            setShowSettings(true);
            setTimeout(() => document.getElementById('dashboard-section')?.scrollIntoView({ behavior: 'smooth' }), 100);
          }} aria-label="Dashboard" data-tooltip="View usage statistics">
            <Sparkles size={14} />
          </button>
          <button className="sidebar-btn" onClick={() => setShowSettings(true)}
            aria-label="Settings" data-tooltip="Open settings">
            <Settings2 size={14} />
          </button>
          <button className="sidebar-btn" onClick={async () => {
            const topics = await invoke<any[]>("get_help_topics");
            const msg = topics.map((t: any, i: number) => `${i+1}. ${t.title}\n   ${t.content}`).join('\n\n');
            alert(msg);
          }} aria-label="Help" data-tooltip="Show help topics">
            ?
          </button>
        </div>

        <div className="sidebar-search" role="search" aria-label="Search sessions">
          <input placeholder="Search sessions..." value={searchQuery} aria-label="Search sessions"
            onChange={e => {
              const v = e.target.value;
              setSearchQuery(v);
              if (searchTimeout.current) clearTimeout(searchTimeout.current);
              if (!v.trim()) { setSearchResults(null); return; }
              searchTimeout.current = window.setTimeout(async () => {
                try {
                  const r = await invoke<Session[]>("search_sessions", { query: v.trim() });
                  setSearchResults(r);
                } catch (_) {}
              }, 250);
            }} />
        </div>

        {searchResults !== null && searchResults.length > 0 && (
          <div className="search-filters">
            <span className="search-filter-label">{searchResults.length} results</span>
          </div>
        )}

        {!ollamaConnected && !isDiscovering && ollamaInstances.length === 0 && (
          <div className="lan-warning">
            <span>Ollama not found locally</span>
          </div>
        )}

        {isDiscovering && (
          <div className="lan-warning lan-discovering">
            <RefreshCw size={14} className="spin" /> Scanning LAN for Ollama...
          </div>
        )}

        {!ollamaIsLocal && ollamaConnected && (
          <div className="lan-warning lan-remote">
            <span>⚠️ Using Ollama on LAN — data is sent over the network</span>
          </div>
        )}

        {ollamaInstances.length > 0 && !ollamaConnected && (
          <div className="lan-instances">
            <span className="lan-label">Ollama found on LAN:</span>
            {ollamaInstances.map((inst, i) => (
              <button key={i} className="lan-instance-btn"
                onClick={() => selectLanOllamaInstance(inst.url)}>
                {inst.label}
              </button>
            ))}
          </div>
        )}

        {updateAvailable && (
          <div className="update-banner">
            <div className="update-info"><Download size={14} /><span>v{updateAvailable.version}</span></div>
            <button className="plaud-btn btn-primary btn-small" onClick={handleUpdate} disabled={isUpdating}>
              {isUpdating ? '...' : 'Update'}
            </button>
          </div>
        )}

        <div className="sidebar-list">
          {(searchResults && searchResults.length === 0) ? (
            <p className="sidebar-empty">No results found.</p>
          ) : !searchResults && sessions.length === 0 ? (
            <p className="sidebar-empty">No sessions yet.</p>
          ) : (
            (searchResults ?? sessions).map((session) => (
              <div key={session.id}
                className={`sidebar-item ${selectedSessionId === session.id ? 'active' : ''}`}
                onClick={() => setSelectedSessionId(session.id)}>
                <h4>{session.title}</h4>
                <div className="sidebar-item-meta">
                  <span className="type">{SESSION_TYPE_LABELS[session.session_type] || session.session_type}</span>
                  {durations[session.id] && <span className="duration">{durations[session.id]}</span>}
                  <span className="time">{new Date(session.created_at).toLocaleDateString()}</span>
                </div>
                {session.tags && (
                  <div className="sidebar-item-tags" style={{ marginTop: '4px', display: 'flex', gap: '4px', flexWrap: 'wrap' }}>
                    {session.tags.split(',').map(t => t.trim()).filter(Boolean).map(t => (
                      <span key={t} className="tag-badge" style={{ fontSize: '0.65rem', padding: '1px 6px', borderRadius: '8px', background: 'rgba(139,92,246,0.2)', color: '#8b5cf6' }}>{t}</span>
                    ))}
                  </div>
                )}
              </div>
            ))
          )}
        </div>

        {/* ── RAG: Ask AI ── */}
        <div className="sidebar-rag">
          <div className="rag-header" onClick={() => setShowRag(!showRag)}
            role="button" tabIndex={0} aria-label="Toggle Ask AI panel"
            onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setShowRag(!showRag); } }}>
            <BrainCircuit size={14} />
            <span>Ask AI</span>
            <ChevronDown size={12} className={`rag-chevron ${showRag ? 'open' : ''}`} />
          </div>
          {showRag && (
            <div className="rag-body" role="region" aria-label="Ask AI about your sessions">
              <textarea className="rag-input" rows={2}
                placeholder="Ask a question about your sessions..."
                value={ragQuery}
                onChange={e => setRagQuery(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleRagQuery(); } }}
                aria-label="Your question" data-tooltip="Ask a question and the AI will search across all your sessions for the answer" />
              <button className="plaud-btn btn-primary btn-small" style={{ width: '100%', marginTop: '0.25rem' }}
                onClick={handleRagQuery} disabled={ragLoading || !ragQuery.trim()}
                aria-label="Ask AI">
                {ragLoading ? <RefreshCw size={14} className="spin" /> : 'Ask'}
              </button>
              {ragAnswer !== null && (
                <div className="rag-answer">
                  <div className="rag-answer-text">{ragAnswer}</div>
                  <button className="plaud-btn btn-outline btn-small" style={{ marginTop: '0.25rem' }}
                    onClick={() => setRagAnswer(null)} aria-label="Clear AI answer">Clear</button>
                </div>
              )}
            </div>
          )}
        </div>
      </aside>
    </div>
  );
}

export default App;
