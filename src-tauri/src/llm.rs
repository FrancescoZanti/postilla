use crate::remote_llm;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::net::Ipv4Addr;

static DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: Option<String>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[derive(Deserialize, Debug)]
struct OllamaModelListResponse {
    models: Vec<OllamaModelItem>,
}

#[derive(Deserialize, Debug)]
pub struct OllamaModelItem {
    pub name: String,
}

#[derive(Serialize, Debug)]
pub struct EvaluatedModel {
    pub name: String,
    pub score: i32,
    pub recommended: bool,
}

#[derive(Serialize, Debug)]
pub struct OllamaInstance {
    pub url: String,
    pub label: String,
    pub is_local: bool,
}

async fn ping_ollama(url: &str) -> bool {
    let client = match Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.get(&format!("{}/api/tags", url)).send().await {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

fn get_local_ip() -> Option<Ipv4Addr> {
    local_ip_address::local_ip().ok().and_then(|ip| {
        match ip {
            std::net::IpAddr::V4(v4) => Some(v4),
            _ => None,
        }
    })
}

/// Scans the /24 subnet of the local IP on port 11434.
async fn scan_subnet() -> Vec<String> {
    let local_ip = match get_local_ip() {
        Some(ip) => ip,
        None => return vec![],
    };

    let base = Ipv4Addr::new(
        local_ip.octets()[0],
        local_ip.octets()[1],
        local_ip.octets()[2],
        0,
    );

    let mut found = Vec::new();
    // Scan in parallel with batches
    let mut handles = Vec::new();
    for host in 1..255 {
        let octets = base.octets();
        let target = format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], host);
        handles.push(tokio::spawn(async move {
            let url = format!("http://{}:11434", target);
            if ping_ollama(&url).await {
                Some(url)
            } else {
                None
            }
        }));
    }

    for handle in handles {
        if let Ok(Some(url)) = handle.await {
            found.push(url);
            // We only need one LAN instance — can break, but let's wait a bit
            // Actually just collect all for now
        }
    }
    found
}

pub async fn discover_ollama_instances() -> Vec<OllamaInstance> {
    let mut instances = Vec::new();

    // 1. Try localhost
    if ping_ollama(DEFAULT_OLLAMA_URL).await {
        instances.push(OllamaInstance {
            url: DEFAULT_OLLAMA_URL.to_string(),
            label: "Localhost".into(),
            is_local: true,
        });
        // If local works, don't bother scanning LAN
        return instances;
    }

    // 2. Scan LAN subnet
    let lan_urls = scan_subnet().await;
    for url in lan_urls {
        let label = url
            .trim_start_matches("http://")
            .trim_end_matches(":11434")
            .to_string();
        instances.push(OllamaInstance {
            url,
            label: format!("LAN — {}", label),
            is_local: false,
        });
    }

    instances
}

pub async fn get_available_models_at(url: &str) -> Result<Vec<EvaluatedModel>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let tags_url = format!("{}/api/tags", url);
    let response = client
        .get(&tags_url)
        .send()
        .await
        .map_err(|e| format!("Impossibile connettersi ad Ollama (è in esecuzione?): {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Errore da Ollama: {}", response.status()));
    }

    let result: OllamaModelListResponse = response
        .json()
        .await
        .map_err(|e| format!("Impossibile parsare la lista dei modelli da Ollama: {}", e))?;

    let mut evaluated_models: Vec<EvaluatedModel> = result.models.into_iter().map(|model| {
        let name = model.name.clone();
        let name_lower = name.to_lowercase();
        let mut score = 0;

        if name_lower.contains("llama3.3") { score += 100; }
        else if name_lower.contains("llama3.2") { score += 90; }
        else if name_lower.contains("llama3.1") { score += 80; }
        else if name_lower.contains("llama3") { score += 70; }
        else if name_lower.contains("qwen2.5") { score += 95; }
        else if name_lower.contains("qwen2") { score += 85; }
        else if name_lower.contains("mistral") { score += 75; }
        else if name_lower.contains("mixtral") { score += 80; }
        else if name_lower.contains("gemma2") { score += 85; }
        else if name_lower.contains("gemma") { score += 70; }

        if name_lower.contains("8b") || name_lower.contains("7b") { score += 10; }
        if name_lower.contains("14b") || name_lower.contains("32b") { score += 20; }

        if !name_lower.contains("instruct") && !name_lower.contains("chat") && (name_lower.contains("llama") || name_lower.contains("qwen")) {
            score -= 30;
        }

        EvaluatedModel {
            name: model.name,
            score,
            recommended: false,
        }
    }).collect();

    evaluated_models.sort_by(|a, b| b.score.cmp(&a.score));

    if let Some(first) = evaluated_models.first_mut() {
        first.recommended = true;
    }

    Ok(evaluated_models)
}

pub async fn get_available_models() -> Result<Vec<EvaluatedModel>, String> {
    get_available_models_at(DEFAULT_OLLAMA_URL).await
}

async fn call_ollama(client: &Client, base_url: &str, model: &str, prompt: &str, system: Option<&str>) -> Result<String, String> {
    let req_body = OllamaRequest {
        model: model.to_string(),
        prompt: prompt.to_string(),
        system: system.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        stream: false,
    };

    let generate_url = format!("{}/api/generate", base_url);
    let response = client
        .post(&generate_url)
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Impossibile connettersi ad Ollama: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Errore da Ollama: {}", response.status()));
    }

    let result: OllamaResponse = response
        .json()
        .await
        .map_err(|e| format!("Impossibile parsare la risposta da Ollama: {}", e))?;

    Ok(result.response)
}

async fn ollama_generate(base_url: &str, model: &str, prompt: &str, system: Option<&str>) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    call_ollama(&client, base_url, model, prompt, system).await
}

// ──────────────────────────────
// Dispatch to any provider
// ──────────────────────────────

pub async fn dispatch_summary(provider: &str, api_key: &str, model: &str, transcript: &str, system_prompt: Option<&str>, ollama_url: &str) -> Result<String, String> {
    let system = system_prompt.unwrap_or(
        "Sei un assistente IA utile e preciso. Leggi la seguente trascrizione e fornisci:\n\
         1. Un riassunto chiaro e conciso del contenuto.\n\
         2. Un elenco dei punti chiave (Key Takeaways) o attività da svolgere (Action Items).\n\n\
         Usa la lingua italiana per la tua risposta."
    );
    let prompt = format!("Trascrizione:\n{}", transcript);

    match provider {
        "openai" => remote_llm::openai_chat(api_key, model, &system, &prompt).await,
        "anthropic" => remote_llm::anthropic_chat(api_key, model, &system, &prompt).await,
        _ => ollama_generate(ollama_url, model, &prompt, Some(&system)).await,
    }
}

pub async fn dispatch_title(provider: &str, api_key: &str, model: &str, transcript: &str, session_type: &str, ollama_url: &str) -> Result<String, String> {
    let system = "Sei un assistente che genera titoli concisi e pertinenti.";
    let prompt = format!(
        "Leggi la seguente trascrizione di una sessione di tipo \"{}\".\n\
         Genera UN SOLO TITOLO breve e descrittivo (massimo 8 parole).\n\
         Il titolo deve essere solo in italiano, senza virgolette, senza prefissi.\n\
         Non scrivere altro, solo il titolo.\n\n\
         Trascrizione:\n{}",
        session_type, transcript
    );

    match provider {
        "openai" => remote_llm::openai_chat(api_key, model, system, &prompt).await,
        "anthropic" => remote_llm::anthropic_chat(api_key, model, system, &prompt).await,
        _ => ollama_generate(ollama_url, model, &prompt, Some(system)).await,
    }
}

pub async fn dispatch_annotate_speakers(provider: &str, api_key: &str, model: &str, transcript: &str, participants: &str, ollama_url: &str) -> Result<String, String> {
    let system = "Sei un assistente che analizza trascrizioni e identifica chi sta parlando.";
    let prompt = format!(
        "Hai una trascrizione di una conversazione. I partecipanti sono: {}.\n\
         Analizza la trascrizione e attribuisci ogni frase o paragrafo alla persona che probabilmente lo ha detto.\n\
         Usa il formato:\n\
         **Nome:** [testo]\n\n\
         Se non sei sicuro, usa **Speaker X:** (es. **Speaker 1:**, **Speaker 2:**, ...).\n\
         Non inventare nomi non presenti nella lista partecipanti.\n\
         Mantieni l'ordine e il contenuto originale della trascrizione.\n\n\
         Partecipanti: {}\n\n\
         Trascrizione:\n{}\n\n\
         Trascrizione con parlanti annotati:",
        participants, participants, transcript
    );

    match provider {
        "openai" => remote_llm::openai_chat(api_key, model, system, &prompt).await,
        "anthropic" => remote_llm::anthropic_chat(api_key, model, system, &prompt).await,
        _ => ollama_generate(ollama_url, model, &prompt, Some(system)).await,
    }
}

pub async fn dispatch_mind_map(provider: &str, api_key: &str, model: &str, transcript: &str, summary: Option<&str>, session_type: &str, ollama_url: &str) -> Result<String, String> {
    let system = "Sei un esperto di organizzazione delle informazioni. Genera mappe mentali chiare, gerarchiche e ben strutturate in Markdown.";
    let context = match summary {
        Some(s) => format!("Riassunto:\n{}\n\nTrascrizione completa:\n{}", s, transcript),
        None => format!("Trascrizione:\n{}", transcript),
    };
    let prompt = format!(
        "Leggi il seguente contenuto di una sessione di tipo \"{}\".\n\
         Genera una mappa mentale strutturata in formato Markdown.\n\n\
         Usa questa struttura:\n\
         # Titolo Centrale\n\
         ## Ramo Principale 1\n\
         ### Sotto-ramo 1.1\n\
         ### Sotto-ramo 1.2\n\
         ## Ramo Principale 2\n\
         ### Sotto-ramo 2.1\n\
         ...\n\n\
         La mappa deve avere 3-6 rami principali, ognuno con sotto-rami dove appropriato.\n\
         Sii gerarchico e logico.\n\n\
         Contenuto:\n{}",
        session_type, context
    );

    match provider {
        "openai" => remote_llm::openai_chat(api_key, model, system, &prompt).await,
        "anthropic" => remote_llm::anthropic_chat(api_key, model, system, &prompt).await,
        _ => ollama_generate(ollama_url, model, &prompt, Some(system)).await,
    }
}

pub async fn dispatch_rag_query(provider: &str, api_key: &str, model: &str, question: &str, context: &str, ollama_url: &str) -> Result<String, String> {
    let system = "Sei un assistente IA specializzato nell'analisi di trascrizioni. Rispondi alle domande basandoti esclusivamente sul contesto fornito. Se il contesto non contiene informazioni sufficienti, dillo chiaramente senza inventare nulla.";
    let prompt = format!(
        "Contesto (trascrizioni di sessioni):\n{}\n\n---\n\nDomanda: {}\n\nFornisci una risposta chiara basata solo sul contesto sopra. Usa la lingua italiana.",
        context, question
    );

    match provider {
        "openai" => remote_llm::openai_chat(api_key, model, system, &prompt).await,
        "anthropic" => remote_llm::anthropic_chat(api_key, model, system, &prompt).await,
        _ => ollama_generate(ollama_url, model, &prompt, Some(system)).await,
    }
}
