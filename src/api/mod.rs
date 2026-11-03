mod models;
mod request;
mod transport;
pub use models::{ModelError, allows, check_model, validate_models};

use crate::{auth, harness};
use axum::{
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use bytes::Bytes;
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    convert::Infallible,
    sync::Arc,
    time::Duration,
};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub struct Config {
    pub keys: auth::Store,
    pub adapters: Vec<harness::Adapter>,
    pub timeout: Duration,
    pub max_concurrent: usize,
    pub allowed_models: Vec<String>,
}
pub struct Server {
    keys: auth::Store,
    adapters: HashMap<String, harness::Adapter>,
    providers: Vec<String>,
    allowed: Vec<String>,
    admission: Arc<auth::Admission>,
    tasks: TaskTracker,
}
impl Server {
    pub fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        anyhow::ensure!(
            (Duration::from_millis(1)..=Duration::from_secs(3600)).contains(&config.timeout)
                && (1..=128).contains(&config.max_concurrent),
            "invalid timeout or concurrency limit"
        );
        let admission = config
            .keys
            .open_admission(config.timeout, config.max_concurrent)?;
        Self::with_admission(config, admission)
    }
    pub fn with_admission(
        config: Config,
        admission: Arc<auth::Admission>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut adapters = HashMap::new();
        for adapter in config.adapters {
            anyhow::ensure!(
                adapters.insert(adapter.id().to_string(), adapter).is_none(),
                "duplicate adapter"
            );
        }
        anyhow::ensure!(!adapters.is_empty(), "at least one adapter is required");
        let providers = adapters.keys().cloned().collect::<Vec<_>>();
        validate_models(&config.allowed_models, &providers)?;
        Ok(Arc::new(Self {
            keys: config.keys,
            adapters,
            providers,
            allowed: config.allowed_models,
            admission,
            tasks: TaskTracker::new(),
        }))
    }
    pub async fn close(&self) {
        self.admission.close().await;
        self.tasks.close();
        self.tasks.wait().await;
    }
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        shutdown: CancellationToken,
    ) -> anyhow::Result<()> {
        let connections = TaskTracker::new();
        loop {
            let accepted = tokio::select! { biased; _ = shutdown.cancelled() => break, accepted = listener.accept() => accepted };
            let (stream, _) = accepted?;
            let server = self.clone();
            let canceled = shutdown.child_token();
            connections.spawn(async move {
                let connection_cancel = canceled.clone();
                let io = TokioIo::new(transport::Socket {
                    stream,
                    canceled: canceled.clone(),
                    write_wait: None,
                });
                let service = service_fn(move |request: Request<Incoming>| {
                    let server = server.clone();
                    let canceled = connection_cancel.child_token();
                    async move {
                        Ok::<_, Infallible>(server.handle(request.map(Body::new), canceled).await)
                    }
                });
                let mut builder = http1::Builder::new();
                builder
                    .timer(TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(5))
                    .max_buf_size(16 * 1024);
                let connection = builder.serve_connection(io, service);
                tokio::pin!(connection);
                tokio::select! { _ = &mut connection => {}, _ = canceled.cancelled() => {} }
                canceled.cancel();
            });
        }
        connections.close();
        connections.wait().await;
        self.close().await;
        Ok(())
    }
    async fn handle(
        self: Arc<Self>,
        request: Request<Body>,
        canceled: CancellationToken,
    ) -> Response<Body> {
        let request_id = random_id();
        let mut response = self.dispatch(request, canceled, &request_id).await;
        let headers = response.headers_mut();
        headers.insert("x-request-id", request_id.parse().unwrap());
        headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        headers.insert("x-content-type-options", "nosniff".parse().unwrap());
        response
    }
    async fn dispatch(
        &self,
        request: Request<Body>,
        canceled: CancellationToken,
        request_id: &str,
    ) -> Response<Body> {
        let (parts, body) = request.into_parts();
        let path = parts.uri.path();
        if path == "/healthz" && parts.method == "GET" {
            return response(200, json!({"status":"ok"}));
        }
        if !path.starts_with("/v1/") {
            return failure(404, "not_found", "Endpoint not found.");
        }
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>();
        if token.len() != 2 || !token[0].eq_ignore_ascii_case("bearer") {
            return invalid_key();
        }
        let token = token[1].to_string();
        let principal = match self
            .keys
            .authenticate_cancellable(token.clone(), canceled.clone())
            .await
        {
            Ok(principal) => principal,
            Err(auth::Error::InvalidKey | auth::Error::ExpiredKey | auth::Error::NotFound) => {
                return invalid_key();
            }
            Err(error) => return admission_error(error),
        };
        if path == "/v1/models" && parts.method == "GET" {
            return self.models(&principal);
        }
        let chat = path == "/v1/chat/completions";
        if !chat && path != "/v1/completions" {
            return failure(404, "not_found", "Endpoint not found.");
        }
        if parts.method != "POST" {
            let mut r = failure(405, "method_not_allowed", "Use POST.");
            r.headers_mut()
                .insert(header::ALLOW, "POST".parse().unwrap());
            return r;
        }
        let media = parts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if media.split(';').next().unwrap_or("").trim() != "application/json" {
            return failure(400, "invalid_content_type", "Use application/json.");
        }
        let body = match tokio::time::timeout(Duration::from_secs(15), to_bytes(body, 1024 * 1024))
            .await
        {
            Ok(Ok(b)) => b,
            Ok(Err(error)) => {
                use std::error::Error;
                if error
                    .source()
                    .is_some_and(|e| e.is::<http_body_util::LengthLimitError>())
                {
                    return failure(413, "body_too_large", "Request exceeds 1 MiB.");
                }
                return failure(400, "invalid_body", "Unable to read request body.");
            }
            Err(_) => return failure(400, "invalid_body", "Unable to read request body."),
        };
        let req = match request::parse(&body, chat) {
            Ok(r) => r,
            Err(message) => return failure(400, "unsupported_request", message),
        };
        if !principal.can("completions:create") {
            return failure(403, "permission_denied", "Key cannot use this model.");
        }
        if let Err(e) = check_model(
            &req.model,
            &principal.models,
            &self.providers,
            &self.allowed,
        ) {
            return if e.code == "permission_denied" {
                failure(403, e.code, "Key cannot use this model.")
            } else {
                failure(
                    404,
                    "model_not_found",
                    "Model is not available on this server.",
                )
            };
        }
        let guard = canceled.clone().drop_guard();
        let lease = match self
            .admission
            .admit(token, req.model.clone(), canceled.clone())
            .await
        {
            Ok(l) => l,
            Err(e) => return admission_error(e),
        };
        let (provider, model) = req.model.split_once('/').unwrap();
        let adapter = self.adapters[provider].clone();
        let input = harness::Request {
            model: model.to_string(),
            prompt: req.prompt,
        };
        let (sender, mut events) = mpsc::channel(8);
        let request_id = request_id.to_string();
        self.tasks.spawn(async move {
            let cancel = lease.cancellation();
            let (delta_tx,delta_rx) = mpsc::channel(8);
            let result = forward_output(adapter.run(input,cancel.clone(),delta_tx), delta_rx, &sender, &cancel).await;
            // The adapter has reaped its child before the lease can be released.
            let result = match (result,lease.cause()) {
                (Ok(usage),None) => Event::Finished(usage),
                (_,cause) => { let e = run_error(cause); eprintln!("{}",json!({"level":"ERROR","msg":"request failed","request_id":request_id,"code":e.1})); Event::Failed(e) },
            };
            let _ = sender.send(result).await;
            drop(lease);
        });
        let id = format!(
            "{}{}",
            if chat { "chatcmpl-" } else { "cmpl-" },
            random_id()
        );
        let created = chrono::Utc::now().timestamp();
        let model = req.model;
        if req.stream {
            let stream = async_stream::stream! {
                let _guard = guard;
                if chat { yield Ok::<Bytes,Infallible>(sse(chunk(&id,created,&model,chat,json!({"role":"assistant","content":""}),false,req.include_usage))); }
                let mut heartbeat = tokio::time::interval(Duration::from_secs(15)); heartbeat.tick().await;
                loop {
                    tokio::select! {
                        event = events.recv() => match event {
                            Some(Event::Text(text)) => yield Ok(sse(chunk(&id,created,&model,chat,json!({"content":text}),false,req.include_usage))),
                            Some(Event::Finished(usage)) => {
                                yield Ok(sse(chunk(&id,created,&model,chat,json!({}),true,req.include_usage)));
                                if req.include_usage && let Some(usage) = usage { yield Ok(sse(json!({"id":id,"created":created,"model":model,"object":if chat {"chat.completion.chunk"} else {"text_completion"},"choices":[],"usage":usage}))); }
                                yield Ok(Bytes::from_static(b"data: [DONE]\n\n")); break;
                            }
                            Some(Event::Failed((status,code,message))) => { yield Ok(sse(error_body(status,code,message))); break; }
                            None => break,
                        },
                        _ = heartbeat.tick() => yield Ok(Bytes::from_static(b": keep-alive\n\n")),
                    }
                }
            };
            return Response::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header("x-accel-buffering", "no")
                .body(Body::from_stream(stream))
                .unwrap();
        }
        let _guard = guard;
        let mut text = String::new();
        while let Some(event) = events.recv().await {
            match event {
                Event::Text(delta) => text.push_str(&delta),
                Event::Failed((status, code, message)) => return failure(status, code, message),
                Event::Finished(usage) => {
                    let mut choice = json!({"index":0,"finish_reason":"stop","logprobs":null});
                    if chat {
                        choice["message"] = json!({"role":"assistant","content":text});
                    } else {
                        choice["text"] = json!(text);
                    }
                    let mut body = json!({"id":id,"created":created,"model":model,"object":if chat {"chat.completion"} else {"text_completion"},"choices":[choice]});
                    if let Some(usage) = usage {
                        body["usage"] = json!(usage);
                    }
                    return response(200, body);
                }
            }
        }
        failure(
            502,
            "harness_failed",
            "Harness failed. Check installation, login, model access, and CLI compatibility.",
        )
    }
    fn models(&self, p: &auth::Principal) -> Response<Body> {
        if !p.can("models:read") {
            return failure(403, "permission_denied", "Key cannot list models.");
        }
        let mut names = BTreeSet::new();
        if !self.allowed.is_empty() {
            for name in &self.allowed {
                if p.allows(name) {
                    names.insert(name.clone());
                }
            }
        } else {
            for provider in &self.providers {
                let alias = format!("{provider}/default");
                if p.allows(&alias) {
                    names.insert(alias);
                }
                for scope in &p.models {
                    if scope.starts_with(&format!("{provider}/")) && !scope.ends_with('*') {
                        names.insert(scope.clone());
                    }
                }
            }
        }
        response(
            200,
            json!({"object":"list","data":names.iter().map(|id|json!({"id":id,"object":"model","created":0,"owned_by":id.split('/').next().unwrap()})).collect::<Vec<_>>()}),
        )
    }
}
async fn forward_output(
    run: impl std::future::Future<Output = Result<Option<harness::Usage>, harness::Error>>,
    mut deltas: mpsc::Receiver<String>,
    sender: &mpsc::Sender<Event>,
    cancel: &CancellationToken,
) -> Result<Option<harness::Usage>, harness::Error> {
    tokio::pin!(run);
    let mut finished = None;
    let mut size = 0usize;
    loop {
        if deltas.is_closed()
            && deltas.is_empty()
            && let Some(result) = finished.take()
        {
            return result;
        }
        tokio::select! {
            biased;
            _ = sender.closed() => {
                cancel.cancel();
                return match finished { Some(result) => result, None => run.await };
            }
            delta = deltas.recv(), if !deltas.is_closed() || !deltas.is_empty() => {
                if let Some(delta) = delta {
                    size = size.saturating_add(delta.len());
                    if size > 8 * 1024 * 1024 {
                        cancel.cancel();
                        if finished.is_none() { let _ = run.await; }
                        return Err(harness::Error::OutputLimit);
                    }
                    tokio::select! {
                        _ = cancel.cancelled() => {},
                        sent = sender.send(Event::Text(delta)) => { if sent.is_err() { cancel.cancel(); } }
                    }
                }
            }
            result = &mut run, if finished.is_none() => finished = Some(result),
        }
    }
}

type RunError = (u16, &'static str, &'static str);
enum Event {
    Text(String),
    Finished(Option<harness::Usage>),
    Failed(RunError),
}
fn random_id() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}
fn response(status: u16, body: Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!("{body}\n")))
        .unwrap()
}
fn error_body(status: u16, code: &str, message: &str) -> Value {
    json!({"error":{"message":message,"type":if status>=500 {"server_error"}else{"invalid_request_error"},"param":null,"code":code}})
}
fn failure(status: u16, code: &str, message: &str) -> Response<Body> {
    response(status, error_body(status, code, message))
}
fn invalid_key() -> Response<Body> {
    let mut r = failure(
        401,
        "invalid_api_key",
        "A valid Bearer API key is required.",
    );
    r.headers_mut()
        .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
    r
}
fn admission_error(e: auth::Error) -> Response<Body> {
    match e {
        auth::Error::Limit { code, retry_after } => {
            let mut r = failure(429, code, "Request limit exceeded.");
            if code != "request_quota_exceeded" {
                r.headers_mut().insert(
                    header::RETRY_AFTER,
                    (retry_after.as_secs_f64().ceil() as u64)
                        .max(1)
                        .to_string()
                        .parse()
                        .unwrap(),
                );
            }
            r
        }
        auth::Error::InvalidKey
        | auth::Error::ExpiredKey
        | auth::Error::RevokedKey
        | auth::Error::NotFound => invalid_key(),
        auth::Error::Canceled | auth::Error::Timeout => {
            failure(408, "request_canceled", "Request was canceled.")
        }
        _ => failure(503, "key_store_error", "Unable to read API key store."),
    }
}
fn run_error(cause: Option<auth::Error>) -> RunError {
    match cause {
        Some(auth::Error::ExpiredKey | auth::Error::RevokedKey | auth::Error::InvalidKey) => (
            401,
            "invalid_api_key",
            "A valid Bearer API key is required.",
        ),
        Some(auth::Error::Timeout) => (
            504,
            "harness_timeout",
            "Harness exceeded the request timeout.",
        ),
        Some(auth::Error::Canceled) => (408, "request_canceled", "Request was canceled."),
        _ => (
            502,
            "harness_failed",
            "Harness failed. Check installation, login, model access, and CLI compatibility.",
        ),
    }
}
fn sse(value: Value) -> Bytes {
    Bytes::from(format!("data: {value}\n\n"))
}
fn chunk(
    id: &str,
    created: i64,
    model: &str,
    chat: bool,
    delta: Value,
    stop: bool,
    usage: bool,
) -> Value {
    let mut choice =
        json!({"index":0,"finish_reason":if stop {json!("stop")}else{Value::Null},"logprobs":null});
    if chat {
        choice["delta"] = delta;
    } else {
        choice["text"] = json!(delta.get("content").and_then(Value::as_str).unwrap_or(""));
    }
    let mut body = json!({"id":id,"created":created,"model":model,"object":if chat {"chat.completion.chunk"}else{"text_completion"},"choices":[choice]});
    if usage {
        body["usage"] = Value::Null;
    }
    body
}

#[cfg(test)]
mod forwarding_tests {
    use super::*;

    #[tokio::test]
    async fn completion_drains_text_produced_in_the_same_poll() {
        let (deltas, rx) = mpsc::channel(8);
        let (sender, mut events) = mpsc::channel(8);
        let run = async move {
            deltas.send("first".into()).await.unwrap();
            deltas.send("last".into()).await.unwrap();
            Ok(None)
        };
        let result = forward_output(run, rx, &sender, &CancellationToken::new()).await;
        assert!(result.is_ok());
        drop(sender);
        let mut text = String::new();
        while let Some(event) = events.recv().await {
            match event {
                Event::Text(delta) => text.push_str(&delta),
                _ => panic!("unexpected event"),
            }
        }
        assert_eq!(text, "firstlast");
    }
}
