use harness_to_api::{api, auth, harness};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

struct Running {
    _dir: tempfile::TempDir,
    store: auth::Store,
    token: String,
    id: String,
    base: String,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}
impl Running {
    async fn start(allowed: Vec<String>) -> Option<Self> {
        let binary = std::env::var("H2A_HARNESS_FIXTURE").ok()?;
        let dir = tempfile::tempdir().unwrap();
        let store = auth::Store::new(dir.path());
        let key = store
            .create(auth::KeySpec {
                name: "api-test".into(),
                models: vec!["codex/*".into()],
                policy: auth::Policy {
                    max_requests: 20,
                    max_concurrent: 1,
                    ..Default::default()
                },
            })
            .unwrap();
        let app = api::Server::new(api::Config {
            keys: store.clone(),
            adapters: vec![
                harness::Adapter::new(harness::Config {
                    provider: harness::Provider::Codex,
                    binary: binary.into(),
                    bob_extension: None,
                })
                .unwrap(),
            ],
            timeout: Duration::from_secs(10),
            max_concurrent: 2,
            allowed_models: allowed,
        })
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = CancellationToken::new();
        let task = tokio::spawn(app.serve(listener, stop.clone()));
        Some(Self {
            _dir: dir,
            store,
            token: key.token,
            id: key.principal.id,
            base,
            stop,
            task,
        })
    }
    async fn end(self) {
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(4), self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    fn post(&self, path: &str, body: Value) -> reqwest::RequestBuilder {
        reqwest::Client::new()
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
    }
    fn accepted(&self) -> u64 {
        self.store.inspect(&self.id).unwrap().accepted
    }
}
fn body(model: &str, stream: bool) -> Value {
    json!({"model":model,"stream":stream,"messages":[{"role":"user","content":"check api"}],"tools":[{"type":"function","function":{"name":"lookup_status","parameters":{"type":"object","properties":{"service":{"type":"string"}},"required":["service"],"additionalProperties":false},"strict":true}}],"tool_choice":"required","parallel_tool_calls":false})
}
#[tokio::test]
async fn bridge_http_round_trip_and_stream() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    let response: Value = s
        .post("/v1/chat/completions", body("codex/default", false))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["choices"][0]["finish_reason"], "tool_calls");
    let message = response["choices"][0]["message"].clone();
    assert_eq!(
        message["tool_calls"][0]["function"]["name"],
        "lookup_status"
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            message["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap()
        )
        .unwrap(),
        json!({"service":"api"})
    );
    let mut follow = body("codex/default", false);
    follow["tool_choice"] = json!("auto");
    follow["messages"]
        .as_array_mut()
        .unwrap()
        .push(message.clone());
    follow["messages"].as_array_mut().unwrap().push(
        json!({"role":"tool","tool_call_id":message["tool_calls"][0]["id"],"content":"operational"}),
    );
    let response: Value = s
        .post("/v1/chat/completions", follow)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        response["choices"][0]["message"]["content"],
        "Service api is operational."
    );
    let stream = s
        .post("/v1/chat/completions", body("codex/default", true))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(stream.contains("tool_calls"));
    assert!(stream.contains("[DONE]"));
    assert!(!stream.contains("H2A_CLIENT_TOOLS_V1"));
    assert_eq!(s.accepted(), 3);
    s.end().await;
}
#[tokio::test]
async fn rejected_requests_and_invalid_generation_do_not_leak() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    for keyword in ["format", "$ref", "oneOf"] {
        let mut b = body("codex/default", false);
        b["tools"][0]["function"]["parameters"][keyword] = json!("unsupported");
        assert_eq!(
            s.post("/v1/chat/completions", b)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let mut b = body("codex/default", false);
    b["messages"] = json!([{"role":"tool","content":"x","tool_call_id":"orphan"}]);
    assert_eq!(
        s.post("/v1/chat/completions", b)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(s.accepted(), 0);
    for model in [
        "codex/bridge-malformed",
        "codex/bridge-duplicate",
        "codex/bridge-unknown",
        "codex/bridge-text",
        "codex/bridge-bad-args",
        "codex/bridge-empty",
        "codex/bridge-parallel",
    ] {
        for stream in [false, true] {
            let response = s
                .post("/v1/chat/completions", body(model, stream))
                .send()
                .await
                .unwrap();
            if !stream {
                assert_eq!(response.status(), 502);
            }
            let text = response.text().await.unwrap();
            assert!(text.contains("invalid_harness_output"));
            assert!(!text.contains("secret"));
            assert!(!text.contains("[DONE]"));
            assert!(!text.contains("lookup_status"));
        }
    }
    assert_eq!(s.accepted(), 14);
    s.end().await;
}

#[tokio::test]
async fn choices_history_and_strict_schema_are_checked_before_admission() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    let mut invalid = body("codex/default", false);
    invalid["tools"][0]["function"]["parameters"]["required"] = json!([]);
    assert_eq!(
        s.post("/v1/chat/completions", invalid)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let mut invalid = body("codex/default", false);
    invalid["tool_choice"] = json!({"type":"function","function":{"name":"other"}});
    assert_eq!(
        s.post("/v1/chat/completions", invalid)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let calls = json!([{ "id":"a", "type":"function", "function":{"name":"old","arguments":"{}"} },{ "id":"b", "type":"function", "function":{"name":"old","arguments":"{}"} }]);
    let mut b = body("codex/default", false);
    b["tool_choice"] = json!("auto");
    b["messages"] = json!([{ "role":"assistant", "content":null, "tool_calls":calls },{ "role":"tool", "tool_call_id":"b", "content":"operational" },{ "role":"tool", "tool_call_id":"a", "content":"operational" }]);
    assert_eq!(
        s.post("/v1/chat/completions", b.clone())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    b["messages"][2]["tool_call_id"] = json!("b");
    assert_eq!(
        s.post("/v1/chat/completions", b)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let mut b = body("codex/bridge-force", false);
    b["tool_choice"] = json!("none");
    assert_eq!(
        s.post("/v1/chat/completions", b)
            .send()
            .await
            .unwrap()
            .status(),
        502
    );
    assert_eq!(s.accepted(), 2);
    s.end().await;
}

#[tokio::test]
async fn buffered_disconnect_cancels_and_releases_capacity() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    let mut socket = TcpStream::connect(s.base.trim_start_matches("http://"))
        .await
        .unwrap();
    let request = body("codex/emit-hang", false).to_string();
    let head = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        s.token,
        request.len(),
        request
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while s.accepted() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(socket);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let r = s
                .post("/v1/chat/completions", body("codex/default", false))
                .send()
                .await
                .unwrap();
            if r.status() == 200 {
                break;
            }
            assert_eq!(r.status(), 429);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(s.accepted(), 2);
    s.end().await;
}

#[tokio::test]
async fn all_fixture_adapters_deliver_validated_tool_calls() {
    let Ok(binary) = std::env::var("H2A_HARNESS_FIXTURE") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("extensions")).unwrap();
    std::fs::write(dir.path().join("extensions/bob.ts"), "").unwrap();
    let store = auth::Store::new(dir.path());
    let key = store
        .create(auth::KeySpec {
            name: "all-providers".into(),
            models: vec!["*".into()],
            policy: auth::Policy::default(),
        })
        .unwrap();
    let adapters = harness::Provider::ALL
        .into_iter()
        .map(|provider| {
            harness::Adapter::new(harness::Config {
                provider,
                binary: binary.clone().into(),
                bob_extension: Some(dir.path().into()),
            })
            .unwrap()
        })
        .collect();
    let app = api::Server::new(api::Config {
        keys: store,
        adapters,
        timeout: Duration::from_secs(10),
        max_concurrent: 2,
        allowed_models: vec![],
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let stop = CancellationToken::new();
    let task = tokio::spawn(app.serve(listener, stop.clone()));
    for provider in harness::Provider::ALL {
        let r = reqwest::Client::new()
            .post(&url)
            .bearer_auth(&key.token)
            .json(&body(&format!("{}/default", provider.id()), false))
            .send()
            .await
            .unwrap();
        let status = r.status();
        let response: Value = r.json().await.unwrap();
        assert_eq!(status, 200, "{provider:?}: {response}");
        assert_eq!(
            response["choices"][0]["finish_reason"], "tool_calls",
            "{provider:?}"
        );
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "lookup_status",
            "{provider:?}"
        );
    }
    stop.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn parallel_calls_stream_with_distinct_ids_and_indexes() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    let mut b = body("codex/bridge-parallel", true);
    b["parallel_tool_calls"] = json!(true);
    b["tool_choice"] = json!({"type":"function","function":{"name":"lookup_status"}});
    let stream = s
        .post("/v1/chat/completions", b)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let events: Vec<Value> = stream
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|data| serde_json::from_str(data).ok())
        .collect();
    let calls: Vec<&Value> = events
        .iter()
        .filter_map(|e| e["choices"][0]["delta"]["tool_calls"].as_array())
        .flatten()
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["index"], 0);
    assert_eq!(calls[1]["index"], 1);
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    assert_eq!(
        calls[0]["function"]["arguments"],
        calls[1]["function"]["arguments"]
    );
    assert!(
        events
            .iter()
            .any(|e| e["choices"][0]["finish_reason"] == "tool_calls")
    );
    assert!(stream.contains("data: [DONE]"));
    assert_eq!(s.accepted(), 1);
    s.end().await;
}
