use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
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

pub async fn get_available_models() -> Result<Vec<EvaluatedModel>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
        
    let response = client
        .get("http://localhost:11434/api/tags")
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

    let mut evaluated_models = Vec::new();

    for model in result.models {
        let name = model.name.clone();
        let name_lower = name.to_lowercase();
        let mut score = 0;
        
        // Punteggi per le attività di riassunto (in italiano, reasoning, ecc)
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
        
        // Modelli piccoli ma buoni
        if name_lower.contains("8b") || name_lower.contains("7b") { score += 10; }
        // Modelli più grandi hanno ragionamento migliore per il riassunto
        if name_lower.contains("14b") || name_lower.contains("32b") { score += 20; }
        
        // Penalizza i modelli non-instruct (non sono chat, ma completamento testuale)
        if !name_lower.contains("instruct") && !name_lower.contains("chat") && (name_lower.contains("llama") || name_lower.contains("qwen")) {
            score -= 30;
        }

        evaluated_models.push(EvaluatedModel {
            name: model.name,
            score,
            recommended: false,
        });
    }

    // Sort by score descending
    evaluated_models.sort_by(|a, b| b.score.cmp(&a.score));

    // Mark the top one as recommended
    if let Some(first) = evaluated_models.first_mut() {
        first.recommended = true;
    }

    Ok(evaluated_models)
}

pub async fn generate_summary(transcript: &str, model: &str) -> Result<String, String> {
    let client = Client::new();
    let prompt = format!(
        "Sei un assistente IA utile e preciso. Leggi la seguente trascrizione e fornisci:\n\
         1. Un riassunto chiaro e conciso del contenuto.\n\
         2. Un elenco dei punti chiave (Key Takeaways) o attività da svolgere (Action Items).\n\n\
         Usa la lingua italiana per la tua risposta.\n\n\
         Trascrizione:\n{}",
        transcript
    );

    let req_body = OllamaRequest {
        model: model.to_string(),
        prompt,
        stream: false,
    };

    let response = client
        .post("http://localhost:11434/api/generate")
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
