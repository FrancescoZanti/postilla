use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct KeygenValidationRequest {
    meta: KeygenValidationMeta,
}

#[derive(Serialize)]
struct KeygenValidationMeta {
    key: String,
    scope: KeygenValidationScope,
}

#[derive(Serialize)]
struct KeygenValidationScope {
    fingerprint: String,
}

#[derive(Deserialize, Debug)]
struct KeygenValidationResponse {
    meta: KeygenValidationResponseMeta,
}

#[derive(Deserialize, Debug)]
struct KeygenValidationResponseMeta {
    valid: bool,
    detail: String,
    code: String,
}

#[derive(Serialize)]
struct MachineActivationRequest {
    data: MachineData,
}

#[derive(Serialize)]
struct MachineData {
    #[serde(rename = "type")]
    type_: String,
    attributes: MachineAttributes,
    relationships: MachineRelationships,
}

#[derive(Serialize)]
struct MachineAttributes {
    fingerprint: String,
    name: String,
}

#[derive(Serialize)]
struct MachineRelationships {
    license: MachineLicenseRelationship,
}

#[derive(Serialize)]
struct MachineLicenseRelationship {
    data: MachineLicenseData,
}

#[derive(Serialize)]
struct MachineLicenseData {
    #[serde(rename = "type")]
    type_: String,
    id: String,
}

const KEYGEN_ACCOUNT_ID: &str = "bb57e8aa-625f-42d2-b5b2-b308796e872e";

pub async fn verify_license(license_key: &str, device_id: &str) -> Result<bool, String> {
    let client = Client::new();
    
    let req_body = KeygenValidationRequest {
        meta: KeygenValidationMeta {
            key: license_key.to_string(),
            scope: KeygenValidationScope {
                fingerprint: device_id.to_string(),
            }
        }
    };

    let validate_url = format!("https://api.keygen.sh/v1/accounts/{}/licenses/actions/validate-key", KEYGEN_ACCOUNT_ID);
    
    let response = client
        .post(&validate_url)
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Errore di rete durante la validazione della licenza: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Errore dal server licenze ({}).", response.status()));
    }
    
    // Convert to text first so we can parse it dynamically to extract the license ID if needed
    let text_response = response.text().await.map_err(|e| e.to_string())?;
    
    // We try to extract the license ID manually using serde_json::Value
    let json_val: serde_json::Value = serde_json::from_str(&text_response).map_err(|e| e.to_string())?;
    let valid = json_val["meta"]["valid"].as_bool().unwrap_or(false);
    let code = json_val["meta"]["code"].as_str().unwrap_or("");
    let detail = json_val["meta"]["detail"].as_str().unwrap_or("");

    if valid {
        return Ok(true);
    }

    if code == "NO_MACHINES" || code == "NO_MACHINE" || code == "FINGERPRINT_MISMATCH" || code == "FINGERPRINT_SCOPE_MISMATCH" || code == "NOT_FOUND" {
        
        // We need the License ID to associate the machine
        let license_id = json_val["data"]["id"].as_str().unwrap_or("");
        
        if license_id.is_empty() {
             return Err(format!("Licenza non valida: {}", detail));
        }

        let act_body = MachineActivationRequest {
            data: MachineData {
                type_: "machines".to_string(),
                attributes: MachineAttributes {
                    fingerprint: device_id.to_string(),
                    name: format!("Device {}", &device_id[..8]),
                },
                relationships: MachineRelationships {
                    license: MachineLicenseRelationship {
                        data: MachineLicenseData {
                            type_: "licenses".to_string(),
                            id: license_id.to_string(),
                        }
                    }
                }
            }
        };

        let machines_url = format!("https://api.keygen.sh/v1/accounts/{}/machines", KEYGEN_ACCOUNT_ID);

        let act_response = client
            .post(&machines_url)
            .header("Content-Type", "application/vnd.api+json")
            .header("Accept", "application/vnd.api+json")
            .header("Authorization", format!("License {}", license_key))
            .json(&act_body)
            .send()
            .await
            .map_err(|e| format!("Errore di rete durante l'attivazione: {}", e))?;

        if act_response.status().is_success() {
            return Ok(true);
        } else {
            let error_text = act_response.text().await.unwrap_or_default();
            if error_text.contains("machine limit") {
                return Err("Hai raggiunto il limite massimo di dispositivi consentiti per questa licenza (3/3).".to_string());
            } else {
                return Err(format!("Impossibile attivare il dispositivo: la licenza potrebbe essere scaduta o non valida."));
            }
        }
    }

    Err(format!("Licenza non valida: {}", detail))
}
