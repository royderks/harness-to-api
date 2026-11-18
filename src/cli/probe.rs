use super::{Flags, args};
use crate::auth;
use serde_json::{Value, json};
use std::{net::IpAddr, time::Duration};
use tokio_util::sync::CancellationToken;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    if args::help(args) {
        super::command_help(
            "probe",
            "  --base-url http://127.0.0.1:8787/v1\n  --model provider/model\n  --tool-call  Verify one client function call without executing it",
        );
        return Ok(());
    }
    let f = Flags::parse(args, &["base-url", "model"], &["tool-call"])?;
    f.only_flags()?;
    let token = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    let result = execute_mode(
        f.get("base-url", "http://127.0.0.1:8787/v1"),
        f.get("model", ""),
        &token,
        CancellationToken::new(),
        f.boolean("tool-call"),
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
    execute_mode(base, model, token, cancel, false).await
}

async fn execute_mode(
    base: &str,
    model: &str,
    token: &str,
    cancel: CancellationToken,
    tool_call: bool,
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
    if tool_call {
        result["capability"] = json!("tool_call");
    }
    let mut request =
        json!({"model":model,"messages":[{"role":"user","content":"Reply with OK."}]});
    if tool_call {
        request["messages"][0]["content"] =
            json!("Call h2a_probe once with an empty object. Do not execute any tools yourself.");
        request["tools"] = json!([{"type":"function","function":{"name":"h2a_probe","description":"Propose a client-side capability check. The client will not execute it.","parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}}}]);
        request["tool_choice"] = json!({"type":"function","function":{"name":"h2a_probe"}});
        request["parallel_tool_calls"] = json!(false);
    }
    let attempt = async {
        let mut response = client
            .post(url)
            .bearer_auth(token)
            .json(&request)
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
            && valid_choice(&body["choices"][0], tool_call);
        result["available"] = json!(valid);
        Some(())
    };
    tokio::select! {_ = cancel.cancelled()=>{},_ = attempt=>{}}
    Ok(result)
}

fn valid_choice(choice: &Value, tool_call: bool) -> bool {
    let message = &choice["message"];
    if message["role"] != "assistant" {
        return false;
    }
    if !tool_call {
        return choice["finish_reason"] == "stop"
            && message["tool_calls"]
                .as_array()
                .is_none_or(|calls| calls.is_empty())
            && message["content"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty());
    }
    if choice["finish_reason"] != "tool_calls" {
        return false;
    }
    let Some(calls) = message["tool_calls"]
        .as_array()
        .filter(|calls| calls.len() == 1)
    else {
        return false;
    };
    let call = &calls[0];
    call["id"].as_str().is_some_and(|id| !id.trim().is_empty())
        && call["type"] == "function"
        && call["function"]["name"] == "h2a_probe"
        && call["function"]["arguments"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .is_some_and(|v| v.as_object().is_some_and(|o| o.is_empty()))
}
