use harness_to_api::{api, auth, harness};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
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
fn chat(model: &str) -> Value {
    json!({"model":model,"messages":[{"role":"user","content":"fixture"}]})
}
#[tokio::test]
async fn authentication_validation_and_catalog_precede_debit() {
    let Some(s) = Running::start(vec!["codex/custom-model".into()]).await else {
        return;
    };
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("{}/healthz", s.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    for path in ["/v1/models", "/v1/unknown"] {
        let response = client
            .get(format!("{}{path}", s.base))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    assert_eq!(
        client
            .get(format!("{}/unrelated", s.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let models: Value = client
        .get(format!("{}/v1/models", s.base))
        .bearer_auth(&s.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(models["data"][0]["id"], "codex/custom-model");
    assert_eq!(models["data"].as_array().unwrap().len(), 1);
    for (body, status) in [
        (chat("codex/default"), 404),
        (chat("claude/default"), 403),
        (
            json!({"model":"codex/custom-model","messages":[{"role":"tool","content":"no"}]}),
            400,
        ),
        (
            json!({"model":"codex/custom-model","messages":[{"role":"user","content":"no","tools":[]}]}),
            400,
        ),
        (json!({"model":"codex/custom-model","prompt":"no"}), 400),
    ] {
        let response = s.post("/v1/chat/completions", body).send().await.unwrap();
        assert_eq!(response.status(), status);
        assert!(response.headers().contains_key("x-request-id"));
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    }
    let response = client
        .post(format!("{}/v1/chat/completions", s.base))
        .bearer_auth(&s.token)
        .header("content-type", "application/json")
        .body(" ".repeat(1024 * 1024 + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    for body in [
        r#"{"model":7,"model":"codex/custom-model","messages":[{"role":"user","content":"x"}]}"#,
        r#"{"model":"codex/custom-model","Model":"codex/custom-model","messages":[{"role":"user","content":"x"}]}"#,
        r#"{"model":"codex/custom-model","messages":[{"role":7,"role":"user","content":"x"}]}"#,
        r#"{"model":"codex/custom-model","messages":[{"role":"user","content":[{"type":"text","text":7,"text":"x"}]}]}"#,
    ] {
        let response = client
            .post(format!("{}/v1/chat/completions", s.base))
            .bearer_auth(&s.token)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(s.accepted(), 0);
    s.end().await;
}
#[tokio::test]
async fn unary_legacy_and_stream_have_terminal_contracts() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    let response = s
        .post("/v1/chat/completions", chat("codex/default"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["message"]["content"], "Hello 🌍");
    assert_eq!(body["usage"]["total_tokens"], 7);
    let body: Value = s
        .post(
            "/v1/completions",
            json!({"model":"codex/default","prompt":"hello"}),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["object"], "text_completion");
    assert_eq!(body["choices"][0]["text"], "Hello 🌍");
    let mut request = chat("codex/default");
    request["stream"] = json!(true);
    request["stream_options"] = json!({"include_usage":true});
    let response = s
        .post("/v1/chat/completions", request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let body = response.text().await.unwrap();
    assert!(body.ends_with("data: [DONE]\n\n"));
    let events = body
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect::<Vec<_>>();
    assert_eq!(events[0]["choices"][0]["delta"]["role"], "assistant");
    assert_eq!(events.last().unwrap()["choices"], json!([]));
    assert_eq!(events.last().unwrap()["usage"]["total_tokens"], 7);
    let mut request = chat("codex/missing-result");
    request["stream"] = json!(true);
    let response = s
        .post("/v1/chat/completions", request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("harness_failed"));
    assert!(!body.contains("[DONE]"));
    assert_eq!(s.accepted(), 4);
    s.end().await;
}
#[tokio::test]
async fn unary_disconnect_cancels_run_and_releases_slot_after_cleanup() {
    let Some(s) = Running::start(vec![]).await else {
        return;
    };
    let mut socket = TcpStream::connect(s.base.trim_start_matches("http://"))
        .await
        .unwrap();
    let body = chat("codex/hang").to_string();
    let head = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        s.token,
        body.len(),
        body
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while s.accepted() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await
        }
    })
    .await
    .unwrap();
    let denied = s
        .post("/v1/chat/completions", chat("codex/default"))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 429);
    drop(socket);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let response = s
                .post("/v1/chat/completions", chat("codex/default"))
                .send()
                .await
                .unwrap();
            if response.status() == 200 {
                break;
            }
            assert_eq!(response.status(), 429);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(s.accepted(), 2);
    s.end().await;
}
#[tokio::test]
async fn admission_usage_is_durable_under_concurrent_requests() {
    let directory = tempfile::tempdir().unwrap();
    let store = auth::Store::new(directory.path());
    let key = store
        .create(auth::KeySpec {
            name: "race".into(),
            models: vec!["*".into()],
            policy: auth::Policy {
                max_requests: 3,
                ..Default::default()
            },
        })
        .unwrap();
    let admission = store.open_admission(Duration::from_secs(5), 128).unwrap();
    let mut jobs = Vec::new();
    for _ in 0..24 {
        let a = Arc::clone(&admission);
        let token = key.token.clone();
        jobs.push(tokio::spawn(async move {
            a.admit(token, "codex/default".into(), CancellationToken::new())
                .await
        }));
    }
    let mut success = 0;
    for job in jobs {
        if let Ok(lease) = job.await.unwrap() {
            success += 1;
            drop(lease)
        }
    }
    assert_eq!(success, 3);
    admission.close().await;
    assert_eq!(
        auth::Store::new(directory.path())
            .inspect(&key.principal.id)
            .unwrap()
            .accepted,
        3
    );
}
