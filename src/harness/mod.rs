use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Provider {
    Codex,
    Antigravity,
    IbmBob,
    Gemini,
    Claude,
    OpenCode,
}
impl Provider {
    pub const ALL: [Self; 6] = [
        Self::Codex,
        Self::Antigravity,
        Self::IbmBob,
        Self::Gemini,
        Self::Claude,
        Self::OpenCode,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
            Self::IbmBob => "ibm-bob",
            Self::Gemini => "gemini",
            Self::Claude => "claude",
            Self::OpenCode => "opencode",
        }
    }
    pub fn binary(self) -> &'static str {
        match self {
            Self::Antigravity => "agy",
            Self::IbmBob => "pi",
            _ => self.id(),
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == s)
    }
}
pub const ALL: [Provider; 6] = Provider::ALL;
#[derive(Clone)]
pub struct Config {
    pub provider: Provider,
    pub binary: PathBuf,
    pub bob_extension: Option<PathBuf>,
}
#[derive(Clone)]
pub struct Adapter {
    config: Config,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub model: String,
    pub prompt: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("harness canceled")]
    Canceled,
    #[error("could not start harness")]
    Spawn,
    #[error("invalid harness response")]
    Protocol,
    #[error("harness output limit exceeded")]
    OutputLimit,
    #[error("harness I/O failed")]
    Io,
}
pub type Result<T> = std::result::Result<T, Error>;
fn executable(path: &Path) -> bool {
    let Ok(m) = path.metadata() else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        m.is_file() && m.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        m.is_file()
    }
}
pub(crate) fn resolve_executable(path: &Path) -> Option<PathBuf> {
    if path.components().count() > 1 || path.is_absolute() {
        return executable(path)
            .then(|| std::fs::canonicalize(path).ok())
            .flatten();
    }
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let p = dir.join(path);
        if executable(&p) {
            return std::fs::canonicalize(p).ok();
        }
        #[cfg(windows)]
        {
            for ext in std::env::var("PATHEXT")
                .unwrap_or(".COM;.EXE;.BAT;.CMD".into())
                .split(';')
            {
                let p = dir.join(format!("{}{}", path.display(), ext));
                if executable(&p) {
                    return std::fs::canonicalize(p).ok();
                }
            }
        }
    }
    None
}
impl Adapter {
    pub fn new(mut config: Config) -> Result<Self> {
        if config.binary.as_os_str().is_empty() {
            config.binary = PathBuf::from(config.provider.binary());
        }
        config.binary = resolve_executable(&config.binary).ok_or(Error::Spawn)?;
        if config.provider == Provider::IbmBob {
            let ext = config.bob_extension.as_ref().ok_or(Error::Spawn)?;
            if !ext.is_absolute() || !ext.join("extensions/bob.ts").is_file() {
                return Err(Error::Spawn);
            }
        }
        Ok(Self { config })
    }
    pub fn id(&self) -> &str {
        self.config.provider.id()
    }
    pub fn provider(&self) -> Provider {
        self.config.provider
    }
    pub async fn run(
        &self,
        req: Request,
        cancel: CancellationToken,
        emit: mpsc::Sender<String>,
    ) -> Result<Option<Usage>> {
        let work = tempfile::Builder::new()
            .prefix("h2a-")
            .tempdir()
            .map_err(|_| Error::Io)?;
        let aux = tempfile::Builder::new()
            .prefix("h2a-config-")
            .tempdir()
            .map_err(|_| Error::Io)?;
        let mut env: Vec<(String, std::ffi::OsString)> = [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SystemRoot",
            "COMSPEC",
            "PATHEXT",
            "TMPDIR",
            "TMP",
            "TEMP",
            "LANG",
            "LC_ALL",
            "CODEX_HOME",
            "XDG_CONFIG_HOME",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "NO_PROXY",
            "SSL_CERT_FILE",
            "NODE_EXTRA_CA_CERTS",
        ]
        .into_iter()
        .filter_map(|k| std::env::var_os(k).map(|v| (k.into(), v)))
        .collect();
        let mut args: Vec<String>;
        let mut input = req.prompt.clone();
        let mut add_env = |names: &[&str]| {
            for &k in names {
                if let Some(v) = std::env::var_os(k) {
                    env.push((k.into(), v));
                }
            }
        };
        match self.provider() {
            Provider::Codex => {
                args = vecs(&[
                    "exec",
                    "--json",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--ignore-user-config",
                    "--ignore-rules",
                    "--sandbox",
                    "read-only",
                    "--disable",
                    "shell_tool",
                    "--disable",
                    "hooks",
                    "-c",
                    "approval_policy=\"never\"",
                    "-c",
                    "web_search=\"disabled\"",
                ]);
                if req.model != "default" {
                    args.extend(vecs(&["--model", &req.model]));
                }
                args.push("-".into());
            }
            Provider::Antigravity => {
                args = vecs(&[
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--sandbox",
                ]);
                if req.model != "default" {
                    args.extend(vecs(&["--model", &req.model]));
                }
                input = format!(
                    "{}\n",
                    json!({"event":"user","message":{"content":req.prompt}})
                );
            }
            Provider::IbmBob => {
                env.retain(|(k, _)| k != "CODEX_HOME");
                for k in [
                    "PI_CODING_AGENT_DIR",
                    "PI_OFFLINE",
                    "IBM_BOB_API_KEY",
                    "IBM_BOB_KEY",
                    "IBM_BOB_API",
                    "IBM_BOB_BASE_URL",
                    "IBM_BOB_AUTH_BASE_URL",
                    "IBM_BOB_AUTH_SCHEME",
                    "IBM_BOB_MODELS",
                    "IBM_BOB_DISCOVER_MODELS",
                    "IBM_BOB_MODEL_DISCOVERY_TIMEOUT_MS",
                    "IBM_BOB_TOKEN_REQUEST_TIMEOUT_MS",
                    "IBM_BOB_INSTANCE_ID",
                    "IBM_BOB_TEAM_ID",
                    "IBM_BOB_READ_BOBSHELL_SETTINGS",
                    "IBM_BOB_HEADERS_JSON",
                    "IBM_BOB_USER_AGENT",
                    "IBM_BOB_INPUT",
                    "IBM_BOB_REASONING",
                    "IBM_BOB_REASONING_MODELS",
                    "IBM_BOB_CONTEXT_WINDOW",
                    "IBM_BOB_MAX_TOKENS",
                    "IBM_BOB_CREDITS_PER_MILLION",
                    "IBM_BOB_SUPPORTS_DEVELOPER_ROLE",
                    "IBM_BOB_SUPPORTS_REASONING_EFFORT",
                    "IBM_BOB_SUPPORTS_USAGE_IN_STREAMING",
                    "IBM_BOB_SUPPORTS_STRICT_MODE",
                    "IBM_BOB_MAX_TOKENS_FIELD",
                    "IBM_BOB_SUPPORTS_EAGER_TOOL_INPUT_STREAMING",
                    "IBM_BOB_SUPPORTS_LONG_CACHE_RETENTION",
                    "IBM_BOB_SUPPORTS_CACHE_CONTROL_ON_TOOLS",
                    "IBM_BOB_FORCE_ADAPTIVE_THINKING",
                    "IBM_BOB_ALLOW_EMPTY_SIGNATURE",
                ] {
                    if let Some(v) = std::env::var_os(k) {
                        env.push((k.into(), v));
                    }
                }
                args = vecs(&[
                    "--mode",
                    "json",
                    "--no-session",
                    "--no-tools",
                    "--no-extensions",
                    "--no-skills",
                    "--no-prompt-templates",
                    "--no-themes",
                    "--no-context-files",
                    "--extension",
                ]);
                args.push(
                    self.config
                        .bob_extension
                        .as_ref()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
                args.extend(vecs(&[
                    "--provider",
                    "ibm-bob",
                    "--model",
                    if req.model == "default" {
                        "premium"
                    } else {
                        &req.model
                    },
                ]));
            }
            Provider::Gemini => {
                env.retain(|(k, _)| k != "CODEX_HOME");
                for k in [
                    "GEMINI_API_KEY",
                    "GOOGLE_API_KEY",
                    "GOOGLE_CLOUD_PROJECT",
                    "GOOGLE_CLOUD_PROJECT_ID",
                    "GOOGLE_CLOUD_LOCATION",
                    "GOOGLE_GENAI_USE_VERTEXAI",
                    "GOOGLE_APPLICATION_CREDENTIALS",
                ] {
                    if let Some(v) = std::env::var_os(k) {
                        env.push((k.into(), v));
                    }
                }
                let name = aux.path().file_name().unwrap().to_string_lossy();
                let settings = json!({"tools":{"core":[],"enableHooks":false,"discoveryCommand":"","callCommand":""},"hooksConfig":{"enabled":false},"mcp":{"allowed":[format!("{name}-no-server")],"serverCommand":""},"experimental":{"enableAgents":false,"codebaseInvestigatorSettings":{"enabled":false},"cliHelpAgentSettings":{"enabled":false}},"skills":{"enabled":false},"context":{"fileName":format!("{name}-no-context"),"loadMemoryFromIncludeDirectories":false}});
                let path = aux.path().join("settings.json");
                std::fs::write(&path, settings.to_string()).map_err(|_| Error::Io)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .map_err(|_| Error::Io)?;
                }
                env.push((
                    "GEMINI_CLI_SYSTEM_SETTINGS_PATH".into(),
                    path.into_os_string(),
                ));
                args = vecs(&[
                    "--output-format",
                    "stream-json",
                    "--extensions",
                    "none",
                    "--approval-mode",
                    "default",
                ]);
                if !req.model.is_empty() && req.model != "default" {
                    args.extend(vecs(&["--model", &req.model]));
                }
            }
            Provider::Claude => {
                add_env(&["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]);
                args = vecs(&[
                    "--print",
                    "--safe-mode",
                    "--setting-sources",
                    "",
                    "--settings",
                    r#"{"disableAllHooks":true}"#,
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                    "--tools",
                    "",
                    "--disallowedTools",
                    "*",
                    "--strict-mcp-config",
                    "--disable-slash-commands",
                    "--no-session-persistence",
                    "--no-chrome",
                ]);
                if !req.model.is_empty() && req.model != "default" {
                    args.extend(vecs(&["--model", &req.model]));
                }
            }
            Provider::OpenCode => {
                env.retain(|(k, _)| k != "CODEX_HOME" && k != "XDG_CONFIG_HOME");
                env.push(("XDG_CONFIG_HOME".into(), aux.path().as_os_str().into()));
                for (k, v) in [
                    ("OPENCODE_DISABLE_PROJECT_CONFIG", "1"),
                    ("OPENCODE_DISABLE_CLAUDE_CODE", "1"),
                    (
                        "OPENCODE_CONFIG_CONTENT",
                        r#"{"plugins":["-*","opencode.*"],"policies":[{"action":"tool.use","resource":"*","effect":"deny"},{"action":"mcp.use","resource":"*","effect":"deny"},{"action":"skill.use","resource":"*","effect":"deny"}],"compaction":{"auto":false}}"#,
                    ),
                ] {
                    env.push((k.into(), v.into()));
                }
                args = vecs(&["run", "--standalone", "--format", "json"]);
                if req.model != "default" {
                    args.extend(vecs(&["--model", &req.model]));
                }
            }
        }
        let mut cmd = Command::new(&self.config.binary);
        cmd.args(args)
            .current_dir(work.path())
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            cmd.process_group(0);
        }
        if cancel.is_cancelled() || emit.is_closed() {
            return Err(Error::Canceled);
        }
        let mut child = ChildGuard::new(cmd.spawn().map_err(|_| Error::Spawn)?, work, aux);
        let pid = child.id();
        let mut stdin = child.stdin.take().ok_or(Error::Io)?;
        let stdout = child.stdout.take().ok_or(Error::Io)?;
        let writer = tokio::spawn(async move {
            let r = stdin.write_all(input.as_bytes()).await;
            drop(stdin);
            r
        });
        let mut parser = Parser::new(
            self.provider(),
            if req.model == "default" {
                "premium".into()
            } else {
                req.model
            },
        );
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        let mut total = 0usize;
        let outcome: Result<()> = loop {
            let buf = tokio::select! { biased; _=cancel.cancelled()=>break Err(Error::Canceled), _=emit.closed()=>break Err(Error::Canceled), r=reader.fill_buf()=>match r {Ok(b)=>b,Err(_)=>break Err(Error::Io)} };
            if buf.is_empty() {
                if !line.is_empty() {
                    match parser.consume(&line) {
                        Ok(chunks) => {
                            for chunk in chunks {
                                if !send(&emit, &cancel, chunk).await {
                                    break;
                                }
                            }
                        }
                        Err(e) => break Err(e),
                    }
                }
                break Ok(());
            }
            let n = buf
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buf.len(), |i| i + 1);
            let end = buf[n - 1] == b'\n';
            let content = if end { n - 1 } else { n };
            total = total.saturating_add(content);
            if total > 16 * 1024 * 1024 || line.len() + content > 8 * 1024 * 1024 {
                break Err(Error::OutputLimit);
            }
            line.extend_from_slice(&buf[..content]);
            reader.consume(n);
            if end {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if !line.is_empty() {
                    let chunks = match parser.consume(&line) {
                        Ok(c) => c,
                        Err(e) => break Err(e),
                    };
                    for chunk in chunks {
                        if !send(&emit, &cancel, chunk).await {
                            break;
                        }
                    }
                    if cancel.is_cancelled() || emit.is_closed() {
                        break Err(Error::Canceled);
                    }
                }
                line.clear();
            }
        };
        if outcome.is_err() {
            kill(pid, &mut child).await;
        }
        drop(reader);
        let status = if outcome.is_err() {
            child.wait().await.map_err(|_| Error::Io)
        } else {
            tokio::select! {
                result = child.wait() => result.map_err(|_| Error::Io),
                _ = cancel.cancelled() => { kill(pid, &mut child).await; let _ = child.wait().await; Err(Error::Canceled) },
                _ = emit.closed() => { kill(pid, &mut child).await; let _ = child.wait().await; Err(Error::Canceled) },
            }
        };
        writer.abort();
        let _ = writer.await;
        child.disarm();
        outcome?;
        if cancel.is_cancelled() || emit.is_closed() {
            return Err(Error::Canceled);
        }
        if !status?.success() {
            return Err(Error::Spawn);
        }
        if !parser.done {
            return Err(Error::Protocol);
        }
        Ok(parser.usage)
    }
}
struct ChildGuard {
    child: Option<tokio::process::Child>,
    #[cfg(unix)]
    pid: Option<u32>,
    dirs: Option<(tempfile::TempDir, tempfile::TempDir)>,
}
impl ChildGuard {
    fn new(child: tokio::process::Child, work: tempfile::TempDir, aux: tempfile::TempDir) -> Self {
        #[cfg(unix)]
        let pid = child.id();
        Self {
            child: Some(child),
            #[cfg(unix)]
            pid,
            dirs: Some((work, aux)),
        }
    }
    fn disarm(&mut self) {
        self.child.take();
    }
}
impl std::ops::Deref for ChildGuard {
    type Target = tokio::process::Child;
    fn deref(&self) -> &Self::Target {
        self.child.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.child.as_mut().unwrap()
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            if let Some(pid) = self.pid {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            let _ = child.start_kill();
            let dirs = self.dirs.take();
            std::thread::spawn(move || {
                let dirs = dirs;
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => {
                        if rt.block_on(child.wait()).is_ok() {
                            drop(dirs);
                        } else {
                            std::mem::forget(dirs);
                        }
                    }
                    Err(_) => {
                        std::mem::forget(dirs);
                    }
                }
            });
        }
    }
}
fn vecs(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}
async fn send(tx: &mpsc::Sender<String>, cancel: &CancellationToken, text: String) -> bool {
    tokio::select! {_=cancel.cancelled()=>false, r=tx.send(text)=>r.is_ok()}
}
async fn kill(pid: Option<u32>, child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = pid {
        unsafe {
            libc::kill(-(id as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
    let _ = child.start_kill();
}
fn normalize_fields(v: &mut Value) {
    const FIELDS: &[&str] = &[
        "type",
        "item",
        "text",
        "usage",
        "input_tokens",
        "output_tokens",
        "event",
        "step_update",
        "step_type",
        "text_delta",
        "result",
        "status",
        "response",
        "total_tokens",
        "message",
        "role",
        "provider",
        "model",
        "stopReason",
        "input",
        "output",
        "cacheRead",
        "cacheWrite",
        "totalTokens",
        "assistantMessageEvent",
        "delta",
        "severity",
        "stats",
        "subtype",
        "is_error",
        "error",
        "is_api_error_message",
        "parent_tool_use_id",
        "content",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
        "part",
        "reason",
        "tokens",
        "reasoning",
        "cache",
        "read",
        "write",
    ];
    match v {
        Value::Object(map) => {
            let old = std::mem::take(map);
            for (key, mut value) in old {
                normalize_fields(&mut value);
                let canonical = FIELDS
                    .iter()
                    .find(|f| f.eq_ignore_ascii_case(&key))
                    .copied()
                    .unwrap_or(&key);
                map.insert(canonical.to_owned(), value);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_fields(item);
            }
        }
        _ => {}
    }
}
fn validate_types(provider: Provider, v: &Value) -> Result<()> {
    fn field(v: &Value, path: &[&str], kind: u8) -> bool {
        let mut cur = v;
        for key in path {
            if cur.is_null() {
                return true;
            }
            if !cur.is_object() {
                return false;
            }
            cur = &cur[*key];
        }
        cur.is_null()
            || match kind {
                0 => cur.is_string(),
                1 => cur.as_i64().is_some(),
                2 => cur.is_boolean(),
                3 => cur.is_object(),
                4 => cur.is_array(),
                _ => false,
            }
    }
    let common: &[(&[&str], u8)] = match provider {
        Provider::Codex => &[
            (&["type"], 0),
            (&["item"], 3),
            (&["item", "type"], 0),
            (&["item", "text"], 0),
            (&["usage"], 3),
            (&["usage", "input_tokens"], 1),
            (&["usage", "output_tokens"], 1),
        ],
        Provider::Antigravity => &[
            (&["event"], 0),
            (&["step_update"], 3),
            (&["step_update", "step_type"], 0),
            (&["step_update", "text_delta"], 0),
            (&["result"], 3),
            (&["result", "status"], 0),
            (&["result", "response"], 0),
            (&["result", "usage"], 3),
            (&["result", "usage", "input_tokens"], 1),
            (&["result", "usage", "output_tokens"], 1),
            (&["result", "usage", "total_tokens"], 1),
        ],
        Provider::IbmBob => &[
            (&["type"], 0),
            (&["message"], 3),
            (&["message", "role"], 0),
            (&["message", "provider"], 0),
            (&["message", "model"], 0),
            (&["message", "stopReason"], 0),
            (&["message", "usage"], 3),
            (&["message", "usage", "input"], 1),
            (&["message", "usage", "output"], 1),
            (&["message", "usage", "cacheRead"], 1),
            (&["message", "usage", "cacheWrite"], 1),
            (&["message", "usage", "totalTokens"], 1),
            (&["assistantMessageEvent"], 3),
            (&["assistantMessageEvent", "type"], 0),
            (&["assistantMessageEvent", "delta"], 0),
        ],
        Provider::Gemini => &[
            (&["type"], 0),
            (&["role"], 0),
            (&["status"], 0),
            (&["severity"], 0),
            (&["stats"], 3),
            (&["stats", "input_tokens"], 1),
            (&["stats", "output_tokens"], 1),
            (&["stats", "total_tokens"], 1),
        ],
        Provider::Claude => &[
            (&["type"], 0),
            (&["subtype"], 0),
            (&["is_error"], 2),
            (&["error"], 0),
            (&["is_api_error_message"], 2),
            (&["parent_tool_use_id"], 0),
            (&["event"], 3),
            (&["event", "type"], 0),
            (&["event", "delta"], 3),
            (&["event", "delta", "type"], 0),
            (&["event", "delta", "text"], 0),
            (&["message"], 3),
            (&["message", "content"], 4),
            (&["usage"], 3),
            (&["usage", "input_tokens"], 1),
            (&["usage", "output_tokens"], 1),
            (&["usage", "cache_read_input_tokens"], 1),
            (&["usage", "cache_creation_input_tokens"], 1),
        ],
        Provider::OpenCode => &[
            (&["type"], 0),
            (&["part"], 3),
            (&["part", "type"], 0),
            (&["part", "text"], 0),
            (&["part", "reason"], 0),
            (&["part", "tokens"], 3),
            (&["part", "tokens", "cache"], 3),
            (&["part", "tokens", "input"], 1),
            (&["part", "tokens", "output"], 1),
            (&["part", "tokens", "reasoning"], 1),
            (&["part", "tokens", "cache", "read"], 1),
            (&["part", "tokens", "cache", "write"], 1),
        ],
    };
    let blocks_valid = provider != Provider::Claude
        || v["message"]["content"].as_array().is_none_or(|blocks| {
            blocks.iter().all(|b| {
                b.is_null() || (b.is_object() && field(b, &["type"], 0) && field(b, &["text"], 0))
            })
        });
    if blocks_valid && common.iter().all(|(path, kind)| field(v, path, *kind)) {
        Ok(())
    } else {
        Err(Error::Protocol)
    }
}
struct Parser {
    provider: Provider,
    model: String,
    done: bool,
    started: bool,
    partial: bool,
    text: String,
    usage: Option<Usage>,
}
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn usage(a: u64, b: u64, t: u64) -> Usage {
    Usage {
        prompt_tokens: a,
        completion_tokens: b,
        total_tokens: t,
    }
}
impl Parser {
    fn new(provider: Provider, model: String) -> Self {
        Self {
            provider,
            model,
            done: false,
            started: false,
            partial: false,
            text: String::new(),
            usage: None,
        }
    }
    fn consume(&mut self, line: &[u8]) -> Result<Vec<String>> {
        let mut v: Value = serde_json::from_slice(line).map_err(|_| Error::Protocol)?;
        normalize_fields(&mut v);
        if !v.is_object() {
            return Err(Error::Protocol);
        }
        validate_types(self.provider, &v)?;
        let mut out = Vec::new();
        let typ = s(&v, "type");
        if self.done && self.provider != Provider::IbmBob {
            return Err(Error::Protocol);
        }
        match self.provider {
            Provider::Codex => match typ {
                "item.completed" => {
                    let x = &v["item"];
                    if s(x, "type") == "agent_message" {
                        out.push(s(x, "text").into());
                    }
                }
                "turn.completed" => {
                    self.done = true;
                    let u = &v["usage"];
                    let a = n(u, "input_tokens");
                    let b = n(u, "output_tokens");
                    self.usage = Some(usage(a, b, a.saturating_add(b)));
                }
                "thread.started" | "turn.started" | "item.started" | "item.updated" => {}
                _ => return Err(Error::Protocol),
            },
            Provider::Antigravity => match s(&v, "event") {
                "init" => {}
                "step_update" => {
                    let x = &v["step_update"];
                    if s(x, "step_type") == "agent_response" && !s(x, "text_delta").is_empty() {
                        let t = s(x, "text_delta");
                        self.text.push_str(t);
                        out.push(t.into());
                    }
                }
                "result" => {
                    let x = &v["result"];
                    if s(x, "status") != "SUCCESS" {
                        return Err(Error::Protocol);
                    }
                    let rest = s(x, "response")
                        .strip_prefix(&self.text)
                        .ok_or(Error::Protocol)?;
                    if !rest.is_empty() {
                        out.push(rest.into());
                    }
                    self.done = true;
                    if x["usage"].is_object() {
                        let u = &x["usage"];
                        self.usage = Some(usage(
                            n(u, "input_tokens"),
                            n(u, "output_tokens"),
                            n(u, "total_tokens"),
                        ));
                    }
                }
                _ => return Err(Error::Protocol),
            },
            Provider::IbmBob => {
                let m = &v["message"];
                match typ {
                    "message_start" if s(m, "role") == "assistant" => {
                        if self.started
                            || self.done
                            || s(m, "provider") != "ibm-bob"
                            || s(m, "model") != self.model
                        {
                            return Err(Error::Protocol);
                        }
                        self.started = true;
                    }
                    "message_update" => {
                        let d = &v["assistantMessageEvent"];
                        if s(d, "type") == "text_delta" {
                            if !self.started || self.done {
                                return Err(Error::Protocol);
                            }
                            out.push(s(d, "delta").into());
                        }
                    }
                    "message_end" if s(m, "role") == "assistant" => {
                        if !self.started
                            || self.done
                            || s(m, "provider") != "ibm-bob"
                            || s(m, "model") != self.model
                            || !matches!(s(m, "stopReason"), "stop" | "length")
                        {
                            return Err(Error::Protocol);
                        }
                        self.done = true;
                        let u = &m["usage"];
                        if u.is_object() {
                            self.usage = Some(usage(
                                n(u, "input")
                                    .saturating_add(n(u, "cacheRead"))
                                    .saturating_add(n(u, "cacheWrite")),
                                n(u, "output"),
                                n(u, "totalTokens"),
                            ));
                        }
                    }
                    "error" | "tool_execution_start" => return Err(Error::Protocol),
                    _ => {}
                }
            }
            Provider::Gemini => {
                if typ.is_empty() {
                    return Err(Error::Protocol);
                }
                match typ {
                    "message" if s(&v, "role") == "assistant" => {
                        if let Some(t) = v["content"].as_str().filter(|t| !t.is_empty()) {
                            out.push(t.into());
                        }
                    }
                    "result" => {
                        if s(&v, "status") != "success" {
                            return Err(Error::Protocol);
                        }
                        self.done = true;
                        let u = &v["stats"];
                        if u.is_object() {
                            self.usage = Some(usage(
                                n(u, "input_tokens"),
                                n(u, "output_tokens"),
                                n(u, "total_tokens"),
                            ));
                        }
                    }
                    "error" if s(&v, "severity") != "warning" => return Err(Error::Protocol),
                    "tool_use" | "tool_result" => return Err(Error::Protocol),
                    _ => {}
                }
            }
            Provider::Claude => {
                if typ.is_empty() {
                    return Err(Error::Protocol);
                }
                match typ {
                    "stream_event" if v["parent_tool_use_id"].is_null() => {
                        let e = &v["event"];
                        if s(e, "type") == "content_block_delta"
                            && s(&e["delta"], "type") == "text_delta"
                        {
                            self.partial = true;
                            out.push(s(&e["delta"], "text").into());
                        }
                    }
                    "assistant" => {
                        if !s(&v, "error").is_empty() || v["is_api_error_message"] == true {
                            return Err(Error::Protocol);
                        }
                        if v["parent_tool_use_id"].is_null() {
                            if let Some(blocks) = v["message"]["content"].as_array() {
                                for b in blocks {
                                    match s(b, "type") {
                                        "tool_use" | "server_tool_use" => {
                                            return Err(Error::Protocol);
                                        }
                                        "text" if !self.partial => out.push(s(b, "text").into()),
                                        _ => {}
                                    }
                                }
                            }
                            self.partial = false;
                        }
                    }
                    "result" => {
                        if s(&v, "subtype") != "success" || v["is_error"] != false {
                            return Err(Error::Protocol);
                        }
                        self.done = true;
                        let u = &v["usage"];
                        if u.is_object() {
                            let a = n(u, "input_tokens")
                                .saturating_add(n(u, "cache_read_input_tokens"))
                                .saturating_add(n(u, "cache_creation_input_tokens"));
                            let b = n(u, "output_tokens");
                            self.usage = Some(usage(a, b, a.saturating_add(b)));
                        }
                    }
                    _ => {}
                }
            }
            Provider::OpenCode => {
                if typ.is_empty() {
                    return Err(Error::Protocol);
                }
                let p = &v["part"];
                match typ {
                    "text" => {
                        if s(p, "type") != "text" {
                            return Err(Error::Protocol);
                        }
                        out.push(s(p, "text").into());
                    }
                    "step_finish" => {
                        if s(p, "type") != "step-finish"
                            || !matches!(s(p, "reason"), "stop" | "length")
                        {
                            return Err(Error::Protocol);
                        }
                        self.done = true;
                        let u = &p["tokens"];
                        if u.is_object() {
                            let a = n(u, "input")
                                .saturating_add(n(&u["cache"], "read"))
                                .saturating_add(n(&u["cache"], "write"));
                            let b = n(u, "output").saturating_add(n(u, "reasoning"));
                            self.usage = Some(usage(a, b, a.saturating_add(b)));
                        }
                    }
                    "error" | "tool_use" => return Err(Error::Protocol),
                    _ => {}
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn six_provider_protocols_and_terminals() {
        let cases = [
            (
                Provider::Codex,
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"hi"}}"#,
                r#"{"type":"turn.completed","usage":{"input_tokens":3,"output_tokens":2}}"#,
            ),
            (
                Provider::Antigravity,
                r#"{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"hi"}}"#,
                r#"{"event":"result","result":{"status":"SUCCESS","response":"hi","usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}}"#,
            ),
            (
                Provider::Gemini,
                r#"{"type":"message","role":"assistant","content":"hi"}"#,
                r#"{"type":"result","status":"success","stats":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}"#,
            ),
            (
                Provider::Claude,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}}"#,
                r#"{"type":"result","subtype":"success","is_error":false,"usage":{"input_tokens":3,"output_tokens":2}}"#,
            ),
            (
                Provider::OpenCode,
                r#"{"type":"text","part":{"type":"text","text":"hi"}}"#,
                r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":3,"output":2}}}"#,
            ),
        ];
        for (provider, delta, terminal) in cases {
            let mut p = Parser::new(provider, "test".into());
            assert_eq!(p.consume(delta.as_bytes()).unwrap(), vec!["hi"]);
            p.consume(terminal.as_bytes()).unwrap();
            assert!(p.done);
            assert_eq!(p.usage.unwrap(), usage(3, 2, 5));
        }
        let mut p = Parser::new(Provider::IbmBob, "test".into());
        p.consume(br#"{"type":"message_start","message":{"role":"assistant","provider":"ibm-bob","model":"test"}}"#).unwrap();
        assert_eq!(p.consume(br#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"hi"}}"#).unwrap(),vec!["hi"]);
        p.consume(br#"{"type":"message_end","message":{"role":"assistant","provider":"ibm-bob","model":"test","stopReason":"stop","usage":{"input":3,"output":2,"totalTokens":5}}}"#).unwrap();
        assert!(p.done);
    }
    #[test]
    fn rejects_unsafe_or_incomplete_terminal_events() {
        for provider in Provider::ALL {
            let mut p = Parser::new(provider, "test".into());
            assert!(p.consume(b"not json").is_err());
            assert!(!p.done);
        }
        let mut p = Parser::new(Provider::Claude, "test".into());
        assert!(
            p.consume(br#"{"type":"result","subtype":"success"}"#)
                .is_err()
        );
        let mut p = Parser::new(Provider::Gemini, "test".into());
        assert!(p.consume(br#"{"type":"tool_use"}"#).is_err());
        let mut p = Parser::new(Provider::Codex, "test".into());
        p.consume(br#"{"type":"turn.completed"}"#).unwrap();
        assert!(p.consume(br#"{"type":"thread.started"}"#).is_err());
    }
    #[test]
    fn protocol_filters_and_schema_validation() {
        let cases: &[(Provider, &[&str], &str)] = &[
            (
                Provider::Codex,
                &[
                    r#"{"type":"item.completed","item":{"type":"command_execution","text":"secret"}}"#,
                ],
                r#"{"type":"turn.failed"}"#,
            ),
            (
                Provider::Antigravity,
                &[
                    r#"{"event":"step_update","step_update":{"step_type":"tool","text_delta":"secret"}}"#,
                ],
                r#"{"event":"result","result":{"status":"FAIL"}}"#,
            ),
            (
                Provider::IbmBob,
                &[
                    r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"secret"}}"#,
                ],
                r#"{"type":"tool_execution_start"}"#,
            ),
            (
                Provider::Gemini,
                &[
                    r#"{"type":"message","role":"assistant","content":{"text":"secret"}}"#,
                    r#"{"type":"error","severity":"warning"}"#,
                ],
                r#"{"type":"tool_use"}"#,
            ),
            (
                Provider::Claude,
                &[
                    r#"{"type":"stream_event","parent_tool_use_id":"child","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"secret"}}}"#,
                ],
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use"}]}}"#,
            ),
            (
                Provider::OpenCode,
                &[r#"{"type":"reasoning","part":{"text":"secret"}}"#],
                r#"{"type":"tool_use"}"#,
            ),
        ];
        for (provider, ignored, error) in cases {
            let mut p = Parser::new(*provider, "test".into());
            for line in *ignored {
                assert!(
                    p.consume(line.as_bytes()).unwrap().is_empty(),
                    "{provider:?}"
                );
            }
            assert!(p.consume(error.as_bytes()).is_err(), "{provider:?}");
        }
        let mut p = Parser::new(Provider::Codex, "test".into());
        assert_eq!(
            p.consume(br#"{"TYPE":"item.completed","ITEM":{"TYPE":"agent_message","TEXT":"hi"}}"#)
                .unwrap(),
            vec!["hi"]
        );
        for (provider, invalid) in [
            (
                Provider::IbmBob,
                r#"{"type":"agent_start","message":{"usage":{"cacheRead":"bad"}}}"#,
            ),
            (
                Provider::Claude,
                r#"{"type":"assistant","message":{"content":[{"type":4}]}}"#,
            ),
            (
                Provider::Claude,
                r#"{"type":"result","usage":{"input_tokens":"bad"}}"#,
            ),
            (
                Provider::OpenCode,
                r#"{"type":"other","part":{"tokens":{"cache":{"read":"bad"}}}}"#,
            ),
        ] {
            assert!(
                Parser::new(provider, "test".into())
                    .consume(invalid.as_bytes())
                    .is_err(),
                "{provider:?}"
            );
        }
        let mut p = Parser::new(Provider::Antigravity, "test".into());
        p.consume(br#"{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"first"}}"#).unwrap();
        assert!(
            p.consume(
                br#"{"event":"result","result":{"status":"SUCCESS","response":"different"}}"#
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn fixture_completion_and_cancellation() {
        let Ok(binary) = std::env::var("H2A_HARNESS_FIXTURE") else {
            return;
        };
        let adapter = Adapter::new(Config {
            provider: Provider::Codex,
            binary: binary.into(),
            bob_extension: None,
        })
        .unwrap();
        let (tx, mut rx) = mpsc::channel(4);
        let result = adapter
            .run(
                Request {
                    model: "default".into(),
                    prompt: "hello".into(),
                },
                CancellationToken::new(),
                tx,
            )
            .await
            .unwrap();
        assert_eq!(rx.recv().await.as_deref(), Some("Hello 🌍"));
        assert_eq!(result.unwrap(), usage(5, 2, 7));
        let (tx, _rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        let handle = tokio::spawn(async move {
            adapter
                .run(
                    Request {
                        model: "hang".into(),
                        prompt: "hello".into(),
                    },
                    c,
                    tx,
                )
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        cancel.cancel();
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(3), handle)
                .await
                .unwrap()
                .unwrap(),
            Err(Error::Canceled)
        ));
    }
}
