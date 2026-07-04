use reqwest::Client;
use serde_json::Value;

#[tokio::main]
async fn main() {
    let client = Client::new();
    let account_id = "bb57e8aa-625f-42d2-b5b2-b308796e872e";
    let license_key = "526E66-2F3B09-0FF0A6-3ABDF2-EAE80D-V3";
    let device_id = "test-fingerprint-123";

    let req_body = serde_json::json!({
        "meta": {
            "key": license_key,
            "scope": {
                "fingerprint": device_id
            }
        }
    });

    let validate_url = format!("https://api.keygen.sh/v1/accounts/{}/licenses/actions/validate-key", account_id);
    
    let response = client
        .post(&validate_url)
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .json(&req_body)
        .send()
        .await
        .unwrap();

    let text = response.text().await.unwrap();
    println!("Response: {}", text);
}
