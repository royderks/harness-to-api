use harness_to_api::{api, auth, harness};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::TcpListener;
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
        json!({"role":"tool","tool_call_id":message["tool_calls"][0]["id"],"content":"healthy"}),
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
        "Service api is healthy."
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
    assert_eq!(s.accepted(), 8);
    s.end().await;
}
