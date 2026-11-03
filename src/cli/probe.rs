use super::{Flags, args};
use crate::auth;
use serde_json::{Value, json};
use std::{net::IpAddr, time::Duration};
use tokio_util::sync::CancellationToken;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    if args::help(args) {
        super::command_help(
            "probe",
            "  --base-url http://127.0.0.1:8787/v1\n  --model provider/model",
        );
        return Ok(());
    }
    let f = Flags::parse(args, &["base-url", "model"], &[])?;
    f.only_flags()?;
    let token = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    let result = execute(
        f.get("base-url", "http://127.0.0.1:8787/v1"),
        f.get("model", ""),
        &token,
        CancellationToken::new(),
    )
    .await?;
    println!("{result}");
    anyhow::ensure!(
        result["available"] == true,
        "probe failed; check gateway, credentials, model access, and limits"
    );
    Ok(())
}
pub async fn execute(
    base: &str,
    model: &str,
    token: &str,
    cancel: CancellationToken,
) -> anyhow::Result<Value> {
    anyhow::ensure!(auth::valid_model(model), "--model must be provider/model");
    let mut url = reqwest::Url::parse(base).map_err(|_| anyhow::anyhow!("invalid probe URL"))?;
    anyhow::ensure!(
        url.host().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && ["http", "https"].contains(&url.scheme()),
        "--base-url must be an HTTP(S) URL without credentials, query or fragment"
    );
    if url.scheme() == "http" {
        anyhow::ensure!(
            url.host_str()
                .unwrap()
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback()),
            "plain HTTP is allowed only for a loopback IP"
        );
    }
    anyhow::ensure!(!token.is_empty(), "OPENAI_API_KEY is required");
    eprintln!("Probe sends a real completion request and may consume quota.");
    url.set_path(&format!(
        "{}/chat/completions",
        url.path().trim_end_matches('/')
    ));
    let checked = chrono::Utc::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut result = json!({"checked_at":checked,"model":model,"available":false});
    let attempt = async {
        let mut response = client
            .post(url)
            .bearer_auth(token)
            .json(&json!({"model":model,"messages":[{"role":"user","content":"Reply with OK."}]}))
            .send()
            .await
            .ok()?;
        result["http_status"] = json!(response.status().as_u16());
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes).ok()?;
        let valid = response.status() == 200
            && body.get("error").is_none()
            && body["object"] == "chat.completion"
            && body["model"] == model
            && body["choices"].as_array().is_some_and(|c| c.len() == 1)
            && body["choices"][0]["finish_reason"] == "stop"
            && body["choices"][0]["message"]["role"] == "assistant"
            && body["choices"][0]["message"]["content"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty());
        result["available"] = json!(valid);
        Some(())
    };
    tokio::select! {_ = cancel.cancelled()=>{},_ = attempt=>{}}
    Ok(result)
}
