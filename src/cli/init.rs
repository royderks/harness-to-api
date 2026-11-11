use super::{
    adapters,
    args::{self, Flags},
    find_binary, harness_setup, providers, signal, state,
};
use crate::{api, auth, harness, initprofile};
use serde_json::{Value, json};
use std::{
    io::{BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

type Result<T> = std::result::Result<T, Failure>;
struct Failure {
    status: i32,
    code: &'static str,
    next: String,
}
fn fail(status: i32, code: &'static str, next: impl Into<String>) -> Failure {
    Failure {
        status,
        code,
        next: next.into(),
    }
}
fn io_error<E>(_: E) -> Failure {
    fail(
        1,
        "io_error",
        "Check private local storage and retry; interrupted setup may require h2a init repair.",
    )
}
fn arguments<E>(_: E) -> Failure {
    fail(
        2,
        "invalid_arguments",
        "Run h2a init --help (or h2a init repair --help) for supported flags.",
    )
}
struct Output {
    json: bool,
}
impl Output {
    fn emit(&self, event: &str, mut fields: Value) -> Result<()> {
        fields["schema_version"] = json!(1);
        fields["event"] = json!(event);
        if self.json {
            println!("{fields}");
            std::io::stdout().flush().map_err(io_error)?;
            return Ok(());
        }
        let s = |name: &str| fields[name].as_str().unwrap_or("");
        match event {
            "plan" => println!(
                "Plan: {}; {}; model {}\nState: {}\nClient credential: {}\nKey: 1h from creation, 20 accepted requests, 90s/request, 5/minute, concurrency 1.\nForeground until Ctrl+C. Limits are not provider dollar limits.\nBinary found; login unknown; model access unknown.",
                s("provider"),
                s("endpoint"),
                s("model"),
                s("state_dir"),
                s("credential_path")
            ),
            "ready" => println!(
                "Ready (unverified): {}; model {}\nKey ID: {}; expires {}; remaining {}\nRead the protected credential file in your client: {}\nPython client example:\n{}\nStop: Ctrl+C. Revoke: h2a keys revoke --state-dir {} {}",
                s("endpoint"),
                s("model"),
                s("key_id"),
                s("expires_at"),
                fields["remaining_requests"],
                s("credential_path"),
                s("client_example"),
                fields["state_dir"],
                s("key_id")
            ),
            "probe" => println!(
                "Probe: {} (one quota-counted request; account access is evidence for this request only).",
                s("verification")
            ),
            "stopped" => println!("Stopped: {}.", s("reason")),
            "error" if s("code") == "harness_missing" => eprintln!("\n{}", s("next_step")),
            "error" => eprintln!("h2a init: {}; {}", s("code"), s("next_step")),
            _ => {}
        }
        std::io::stdout().flush().map_err(io_error)
    }
}
struct Prompts {
    lines: tokio::sync::mpsc::UnboundedReceiver<std::io::Result<String>>,
}
impl Prompts {
    fn new() -> Self {
        let (tx, lines) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { lines }
    }
    async fn ask(
        &mut self,
        cancel: &CancellationToken,
        label: &str,
        fallback: &str,
    ) -> Result<String> {
        eprint!("{label} [{fallback}]: ");
        std::io::stderr().flush().map_err(io_error)?;
        let answer = tokio::select! {_ = cancel.cancelled()=>return Err(fail(130,"cancelled","Setup cancelled before readiness.")),answer=self.lines.recv()=>answer};
        match answer {
            Some(Ok(line)) => {
                let line = line.trim();
                Ok(if line.is_empty() {
                    fallback.to_string()
                } else {
                    line.to_string()
                })
            }
            _ => Err(fail(
                2,
                "input_required",
                "Use explicit --provider, --model, --state-dir, --port and --probe or --no-probe flags.",
            )),
        }
    }
}
fn yes(value: &str) -> bool {
    value.eq_ignore_ascii_case("y") || value.eq_ignore_ascii_case("yes")
}
pub async fn run(args: &[String]) -> i32 {
    let mut output = Output { json: false };
    for arg in args.iter().take_while(|a| a.as_str() != "--") {
        let name = arg.trim_start_matches('-');
        if name == "json" {
            output.json = true;
        }
        if let Some(value) = name.strip_prefix("json=") {
            match value {
                "1" | "t" | "T" | "true" | "TRUE" | "True" => output.json = true,
                "0" | "f" | "F" | "false" | "FALSE" | "False" => output.json = false,
                _ => {}
            }
        }
    }
    if args::help(args) {
        if args.first().is_some_and(|s| s == "repair") {
            println!(
                "Usage: h2a init repair [flags]\n\n  --state-dir PATH\n  --discard\n  --yes\n  --json"
            );
        } else {
            println!(
                "Usage: h2a init [flags]\n\n  --provider codex|antigravity|ibm-bob|gemini|claude|opencode\n  --model provider/model\n  --state-dir PATH\n  --port 8787\n  --probe\n  --no-probe\n  --pi-bob-extension PATH\n  --json"
            );
        }
        return 0;
    }
    let cancel = signal();
    let result = if args.first().is_some_and(|s| s == "repair") {
        repair(&args[1..], &output, cancel).await
    } else {
        start(args, &output, cancel).await
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            let _ = output.emit("error", json!({"code":e.code,"next_step":e.next}));
            e.status
        }
    }
}
async fn start(args: &[String], output: &Output, cancel: CancellationToken) -> Result<()> {
    let f = Flags::parse(
        args,
        &["provider", "model", "state-dir", "port", "pi-bob-extension"],
        &["probe", "no-probe", "json"],
    )
    .map_err(arguments)?;
    f.only_flags().map_err(arguments)?;
    let interactive = std::io::stdin().is_terminal() && !output.json;
    if f.seen("probe") && f.seen("no-probe")
        || f.seen("probe") && !f.boolean("probe")
        || f.seen("no-probe") && !f.boolean("no-probe")
    {
        return Err(fail(
            2,
            "probe_choice",
            "Choose exactly one of --probe or --no-probe.",
        ));
    }
    if !interactive {
        if ["provider", "model", "state-dir", "port"]
            .iter()
            .any(|key| !f.seen(key))
        {
            return Err(fail(
                2,
                "input_required",
                "Supply --provider, --model, --state-dir, --port and --probe or --no-probe.",
            ));
        }
        if !f.boolean("probe") && !f.boolean("no-probe") {
            return Err(fail(
                2,
                "probe_choice",
                "Choose --no-probe for offline setup or --probe to consume one request.",
            ));
        }
    }
    let mut provider = f.get("provider", "").to_string();
    let mut model = f.get("model", "").to_string();
    let mut path = state(&f);
    let mut port = f.get("port", "8787").to_string();
    let mut bob = f.get("pi-bob-extension", "").to_string();
    let mut prompts = interactive.then(Prompts::new);
    if let Some(prompts) = &mut prompts {
        if provider.is_empty() {
            eprintln!("Which harness would you like to add?");
            for (index, candidate) in harness::Provider::ALL.into_iter().enumerate() {
                eprintln!(
                    "  {}. {} ({})",
                    index + 1,
                    harness_setup::name(candidate),
                    candidate.id()
                );
            }
            let default = harness::Provider::ALL
                .into_iter()
                .find(|p| find_binary(p.binary()).is_some())
                .unwrap_or(harness::Provider::Codex);
            loop {
                let answer = prompts
                    .ask(&cancel, "Harness (number or name)", default.id())
                    .await?;
                if let Some(chosen) = harness_setup::choice(&answer) {
                    provider = chosen.id().to_string();
                    break;
                }
                eprintln!("Choose a number from 1 to 6 or a harness name.");
            }
        }
        let chosen = harness::Provider::parse(&provider).ok_or_else(|| {
            fail(
                2,
                "invalid_provider",
                "Choose exactly one supported provider; see h2a init --help.",
            )
        })?;
        if find_binary(chosen.binary()).is_none() {
            return Err(fail(
                3,
                "harness_missing",
                harness_setup::instructions(chosen, cfg!(windows)),
            ));
        }
        if model.is_empty() {
            model = prompts
                .ask(&cancel, "Model", &format!("{provider}/default"))
                .await?;
        }
        if !f.seen("port") {
            port = prompts.ask(&cancel, "Port", "8787").await?;
        }
        if !f.seen("state-dir") {
            path = PathBuf::from(
                prompts
                    .ask(&cancel, "State directory", &path.to_string_lossy())
                    .await?,
            );
        }
        if provider == "ibm-bob" && bob.is_empty() {
            bob = prompts
                .ask(&cancel, "Community pi-bob package (absolute path)", "")
                .await?;
        }
    }
    let selected = providers(&provider).map_err(|_| {
        fail(
            2,
            "invalid_provider",
            "Choose exactly one supported provider; see h2a init --help.",
        )
    })?;
    if selected.len() != 1 {
        return Err(fail(
            2,
            "invalid_provider",
            "Choose exactly one supported provider; see h2a init --help.",
        ));
    }
    if provider != "ibm-bob" && !bob.is_empty() {
        return Err(fail(
            2,
            "invalid_extension",
            "Use --pi-bob-extension only with --provider ibm-bob.",
        ));
    }
    if api::check_model(
        &model,
        std::slice::from_ref(&model),
        std::slice::from_ref(&provider),
        &[],
    )
    .is_err()
    {
        return Err(fail(
            2,
            "invalid_model",
            "Use an exact provider/model for the selected provider; wildcards are not allowed.",
        ));
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| fail(2, "invalid_port", "Use a port from 1 to 65535."))?;
    if path.as_os_str().is_empty() {
        return Err(fail(
            2,
            "invalid_state",
            "Choose a private local state directory.",
        ));
    }
    if !path.is_absolute() {
        path = std::env::current_dir().map_err(io_error)?.join(path);
    }
    let adapter_flags = Flags::parse(
        &["--pi-bob-extension".into(), bob.clone()],
        &["pi-bob-extension"],
        &[],
    )
    .map_err(arguments)?;
    let adapters = adapters(&selected, &adapter_flags).map_err(|_| {
        fail(
            3,
            "harness_missing",
            harness_setup::instructions(selected[0], cfg!(windows)),
        )
    })?;
    let endpoint = format!("http://127.0.0.1:{port}/v1");
    let mut probe = f.boolean("probe");
    output.emit("plan",json!({"provider":provider,"endpoint":endpoint,"model":model,"state_dir":path,"credential_path":path.join("init/client.key"),"lifetime":"foreground","key_ttl":"1h","max_requests":20,"request_timeout":"90s","requests_per_minute":5,"max_concurrent":1,"binary":"found","login":"unknown","model_access":"unknown","probe_requested":probe}))?;
    if let Some(prompts) = &mut prompts {
        if !yes(&prompts
            .ask(&cancel, "Start or resume this setup? y/N", "n")
            .await?)
        {
            return output.emit("stopped", json!({"reason":"declined"}));
        }
        if !f.seen("probe") && !f.seen("no-probe") {
            probe = yes(&prompts
                .ask(&cancel, "Run a real probe (consumes quota)? y/N", "n")
                .await?);
        }
    }
    if cancel.is_cancelled() {
        return Err(fail(130, "cancelled", "Setup cancelled before readiness."));
    }
    let files=initprofile::Store::open(&path).map_err(|_|fail(3,"unsafe_storage","Use a private local state directory with owner-only access, regular files and no symbolic links; do not reuse an unsafe destination."))?;
    let keys = files.auth_store();
    let admission=keys.open_admission(Duration::from_secs(90),1).map_err(|_|fail(3,"state_unavailable","Stop the gateway already using this state directory, or check its private local storage; do not remove lock files."))?;
    let listener=tokio::net::TcpListener::bind(("127.0.0.1",port)).await.map_err(|_|fail(3,"port_in_use","The loopback port is unavailable. Stop your own listener or select a free port for a fresh setup; no existing process was contacted."))?;
    let loaded=files.load().map_err(|_|fail(3,"repair_required","Onboarding files are corrupt or unsafe. Inspect the private profile and key ID; repair only identified onboarding artifacts, then rerun init."))?;
    let (mut profile, token) = match loaded {
        Some((p, t)) => {
            if p.provider != provider
                || p.model != model
                || p.port != port
                || p.bob_extension != bob
            {
                return Err(fail(
                    3,
                    "configuration_mismatch",
                    "Reuse the original provider, model, port and extension, or stop and run h2a init repair --state-dir PATH --discard --yes.",
                ));
            }
            (p, t)
        }
        None => {
            let now = chrono::Utc::now();
            let prepared = auth::prepare(
                auth::KeySpec {
                    name: "onboarding".into(),
                    models: vec![model.clone()],
                    policy: auth::Policy {
                        expires_at: Some(now + chrono::Duration::hours(1)),
                        request_timeout: auth::DurationValue(90_000_000_000),
                        max_requests: 20,
                        requests_per_minute: 5,
                        max_concurrent: 1,
                    },
                },
                now,
            )
            .map_err(io_error)?;
            let p = initprofile::Profile {
                version: 1,
                phase: initprofile::Phase::Pending,
                provider: provider.clone(),
                model: model.clone(),
                port,
                bob_extension: bob,
                principal: prepared.principal.clone(),
            };
            files.save_profile(&p).map_err(io_error)?;
            files.create_credential(&prepared.token).map_err(io_error)?;
            keys.insert_prepared(&prepared).map_err(io_error)?;
            (p, prepared.token)
        }
    };
    let inspection=initprofile::resume(&keys,&profile,&token).map_err(|e|{
        let code=match e {initprofile::Error::Expired=>"key_expired",initprofile::Error::Exhausted=>"key_exhausted",initprofile::Error::Missing=>"key_missing",_=>"repair_required"};
        fail(3,code,"Stop and run h2a init repair --state-dir PATH --discard --yes; a repeat never replaces missing or revoked credentials or resets key limits.")
    })?;
    if profile.phase != initprofile::Phase::Ready {
        profile.phase = initprofile::Phase::Ready;
        files.save_profile(&profile).map_err(io_error)?;
    }
    if cancel.is_cancelled() {
        return Err(fail(
            130,
            "cancelled",
            "Setup cancelled before readiness; rerun init to resume the same key.",
        ));
    }
    let app = api::Server::with_admission(
        api::Config {
            keys,
            adapters,
            timeout: Duration::from_secs(90),
            max_concurrent: 1,
            allowed_models: vec![],
        },
        admission,
    )
    .map_err(io_error)?;
    let serving_cancel = cancel.child_token();
    let stop_on_exit = cancel.clone();
    let server_cancel = serving_cancel.clone();
    let serving = tokio::spawn(async move {
        let result = app.serve(listener, server_cancel).await;
        stop_on_exit.cancel();
        result
    });
    let outcome=async {
        output.emit("ready",json!({"provider":provider,"endpoint":endpoint,"model":model,"state_dir":path,"key_id":profile.principal.id,"credential_path":files.credential_path(),"expires_at":profile.principal.policy.expires_at,"remaining_requests":inspection.remaining_requests,"verification":"unverified","client_example":client_example(&files.credential_path(),&endpoint,&model)}))?;
        if probe {
            let result=super::probe::execute(&endpoint,&model,&token,cancel.clone()).await.map_err(io_error)?;
            if cancel.is_cancelled(){return output.emit("stopped",json!({"reason":"signal"}))}
            let verified=result["available"]==true;
            output.emit("probe",json!({"verification":if verified{"verified"}else{"unverified"},"checked_at":result["checked_at"],"http_status":result.get("http_status").cloned().unwrap_or(json!(0)),"model":model}))?;
            if !verified{return Err(fail(4,"probe_failed","Check the official harness login, model access and key limits; rerun with --no-probe to start unverified. An admitted probe consumes quota even on failure."))}
        }
        cancel.cancelled().await;
        output.emit("stopped",json!({"reason":"signal"}))
    }.await;
    serving_cancel.cancel();
    let served = serving.await;
    if outcome.is_ok() && !matches!(served, Ok(Ok(()))) {
        return Err(fail(
            1,
            "server_failed",
            "The listener stopped unexpectedly; check local resources and rerun init to resume.",
        ));
    }
    outcome
}
fn client_example(path: &Path, endpoint: &str, model: &str) -> String {
    format!(
        "import json, pathlib, urllib.request\nkey = pathlib.Path({}).read_text().strip()\nbody = json.dumps({{'model': {}, 'messages': [{{'role': 'user', 'content': 'Say hello.'}}]}}).encode()\nrequest = urllib.request.Request({}, data=body, headers={{'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'}})\nwith urllib.request.urlopen(request) as response:\n    print(response.read().decode())",
        json!(path.to_string_lossy()),
        json!(model),
        json!(format!("{endpoint}/chat/completions"))
    )
}
async fn repair(args: &[String], output: &Output, cancel: CancellationToken) -> Result<()> {
    let f = Flags::parse(args, &["state-dir"], &["discard", "yes", "json"]).map_err(arguments)?;
    f.only_flags().map_err(arguments)?;
    if !f.boolean("discard") {
        return Err(fail(
            2,
            "discard_required",
            "Use --discard to explicitly revoke and remove this onboarding setup.",
        ));
    }
    let consent = if f.boolean("yes") {
        initprofile::ExplicitConsent::Flag
    } else {
        if output.json || !std::io::stdin().is_terminal() {
            return Err(fail(
                2,
                "confirmation_required",
                "Use --discard --yes to explicitly confirm repair without prompts.",
            ));
        }
        if !yes(&Prompts::new()
            .ask(&cancel, "Revoke and discard onboarding? y/N", "n")
            .await?)
        {
            return output.emit("stopped", json!({"reason":"declined"}));
        }
        initprofile::ExplicitConsent::TerminalConfirmation
    };
    let state = state(&f);
    if state.as_os_str().is_empty() {
        return Err(fail(
            2,
            "invalid_state",
            "Choose the original private state directory.",
        ));
    }
    if cancel.is_cancelled() {
        return Err(fail(130, "cancelled", "Repair cancelled before changes."));
    }
    let files=initprofile::Store::open(&state).map_err(|_|fail(3,"unsafe_storage","Repair requires safe private local storage; inspect permissions and symbolic links without deleting unrelated files."))?;
    let keys = files.auth_store();
    let owner = keys
        .open_admission(Duration::from_secs(90), 1)
        .map_err(|_| {
            fail(
                3,
                "state_unavailable",
                "Stop the gateway using this state before repair; do not remove lock files.",
            )
        })?;
    let result=files.discard(&keys,initprofile::DiscardConfirmed::from_cli(consent)).map_err(|_|fail(3,"repair_required","Cannot establish the onboarding identity. Inspect the private profile and revoke only its verified key ID; do not guess or delete other keys."));
    owner.close().await;
    result?;
    output.emit(
        "stopped",
        json!({"reason":"onboarding discarded; run init for a new bounded key"}),
    )
}
