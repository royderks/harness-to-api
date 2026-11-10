mod args;
mod init;
mod keys;
mod probe;
use crate::{api, auth, harness};
use args::Flags;
use serde_json::json;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const HELP: &str = "h2a — Your coding harness, as an authenticated OpenAI-compatible API.\n\nUsage:\n  h2a init [--provider codex --model codex/default --state-dir PATH --port 8787 --no-probe --json]\n  h2a init repair --state-dir PATH --discard --yes\n  h2a doctor\n  h2a keys create|list|revoke|inspect|explain [flags]\n  h2a probe --model provider/model [--base-url URL]\n  h2a serve [--provider codex[,gemini,...]|all] [--port 8787]\n  h2a share [--via tailscale|funnel|cloudflare] [--port 8787]\n  h2a factory validate|plan|seed|adopt|recover\n  h2a version\n  h2a help [command]\n\nGet started:\n  1. Install and sign in to an official harness CLI (for Codex: codex login).\n  2. h2a init\n  3. Use the printed client example in another terminal; Ctrl+C stops the server.\n\nUse h2a <command> --help for flags. Init stores a protected client credential;\nkeys create prints a new key once to stdout. Keep both secret.\nSet HARNESS_API_STATE_DIR to choose the private local state directory.";

pub async fn run(mut args: Vec<String>) -> i32 {
    if args.first().is_some_and(|s| s == "help") && args.len() > 1 {
        args.remove(0);
        args.push("--help".into());
    }
    if args.first().is_some_and(|s| s == "init") {
        return init::run(&args[1..]).await;
    }
    let result = dispatch(&args).await;
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("h2a: {error}");
            1
        }
    }
}
async fn dispatch(args: &[String]) -> anyhow::Result<()> {
    let Some(command) = args.first().map(String::as_str) else {
        println!("{HELP}");
        return Ok(());
    };
    let rest = &args[1..];
    match command {
        "help" | "--help" | "-h" => {
            anyhow::ensure!(rest.is_empty(), "usage: h2a --help or h2a help COMMAND");
            println!("{HELP}");
            Ok(())
        }
        "version" | "--version" => {
            anyhow::ensure!(rest.is_empty(), "usage: h2a version");
            println!("{}", env!("H2A_VERSION"));
            Ok(())
        }
        "keys" => keys::run(rest),
        "probe" => probe::run(rest).await,
        "serve" => serve(rest).await,
        "doctor" => doctor(rest).await,
        "share" => share(rest),
        "factory" => crate::factory::run(rest),
        _ => anyhow::bail!("unknown command; run h2a --help"),
    }
}
fn state_default() -> PathBuf {
    std::env::var_os("HARNESS_API_STATE_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".harness-to-api")
        })
}
fn state(f: &Flags) -> PathBuf {
    PathBuf::from(f.get("state-dir", &state_default().to_string_lossy()))
}
fn split_list(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(|v| v.trim().to_string()).collect()
    }
}
fn providers(value: &str) -> anyhow::Result<Vec<harness::Provider>> {
    if value == "all" {
        return Ok(harness::Provider::ALL.to_vec());
    }
    let mut seen = HashSet::new();
    value.split(',').map(|p|{let provider=harness::Provider::parse(p.trim()).ok_or_else(||anyhow::anyhow!("invalid provider; choose codex, antigravity, ibm-bob, gemini, claude, opencode, or all"))?;anyhow::ensure!(seen.insert(provider),"repeated provider");Ok(provider)}).collect()
}
fn adapters(selected: &[harness::Provider], f: &Flags) -> anyhow::Result<Vec<harness::Adapter>> {
    selected
        .iter()
        .map(|&p| {
            let flag = if p == harness::Provider::IbmBob {
                "pi-bin".to_string()
            } else {
                format!("{}-bin", p.id())
            };
            let extension = f.get("pi-bob-extension", "");
            harness::Adapter::new(harness::Config {
                provider: p,
                binary: PathBuf::from(f.get(&flag, p.binary())),
                bob_extension: (!extension.is_empty()).then(|| PathBuf::from(extension)),
            })
            .map_err(|_| {
                anyhow::anyhow!(
                    "{} CLI not found or invalid extension; install and sign in first",
                    p.id()
                )
            })
        })
        .collect()
}
fn signal() -> CancellationToken {
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("signal handler");
            tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = terminate.recv()=>{}}
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        cancel.cancel();
    });
    token
}
fn command_help(command: &str, flags: &str) {
    eprintln!("Usage: h2a {command} [flags]\n\nFlags:\n{flags}");
}
async fn serve(args: &[String]) -> anyhow::Result<()> {
    if args::help(args) {
        command_help(
            "serve",
            "  --state-dir PATH\n  --provider codex[,gemini,...]|all\n  --allowed-models provider/model,...\n  --host 127.0.0.1\n  --port 8787\n  --timeout 2m\n  --max-concurrent 4\n  --codex-bin codex\n  --antigravity-bin agy\n  --pi-bin pi\n  --gemini-bin gemini\n  --claude-bin claude\n  --opencode-bin opencode\n  --pi-bob-extension PATH",
        );
        return Ok(());
    }
    let f = Flags::parse(
        args,
        &[
            "state-dir",
            "provider",
            "allowed-models",
            "host",
            "port",
            "timeout",
            "max-concurrent",
            "codex-bin",
            "antigravity-bin",
            "pi-bin",
            "gemini-bin",
            "claude-bin",
            "opencode-bin",
            "pi-bob-extension",
        ],
        &[],
    )?;
    f.only_flags()?;
    let port = f.number("port", "8787", 1, 65535)?;
    let timeout = auth::DurationValue::parse(f.get("timeout", "2m"))?;
    anyhow::ensure!(
        timeout.0 > 0 && timeout.0 <= 3_600_000_000_000,
        "timeout must be greater than zero and no more than 1h"
    );
    let max = f.number("max-concurrent", "4", 1, 128)? as usize;
    let selected = providers(f.get("provider", "codex"))?;
    let allowed = split_list(f.get("allowed-models", ""));
    api::validate_models(
        &allowed,
        &selected.iter().map(|p| p.id().into()).collect::<Vec<_>>(),
    )?;
    let adapters = adapters(&selected, &f)?;
    let keys = auth::Store::new(state(&f));
    anyhow::ensure!(
        !keys.list()?.is_empty(),
        "create a key first: h2a keys create --name local (use the same --state-dir)"
    );
    let app = api::Server::new(api::Config {
        keys,
        adapters,
        timeout: timeout.as_duration(),
        max_concurrent: max,
        allowed_models: allowed,
    })?;
    let listener = tokio::net::TcpListener::bind((f.get("host", "127.0.0.1"), port as u16)).await?;
    let address = listener.local_addr()?;
    eprintln!(
        "Serving {} at http://{address}/v1 (press Ctrl+C to stop)",
        f.get("provider", "codex")
    );
    eprintln!(
        "{}",
        json!({"level":"INFO","msg":"listening","address":address.to_string(),"base_path":"/v1","provider":f.get("provider","codex")})
    );
    app.serve(listener, signal()).await
}
fn find_binary(binary: &str) -> Option<PathBuf> {
    let path = Path::new(binary);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        let file = dir.join(binary);
        if file.is_file() {
            Some(file)
        } else if cfg!(windows) && file.with_extension("exe").is_file() {
            Some(file.with_extension("exe"))
        } else {
            None
        }
    })
}
async fn doctor(args: &[String]) -> anyhow::Result<()> {
    if args::help(args) {
        command_help(
            "doctor",
            "  --pi-bin pi\n  --gemini-bin gemini\n  --claude-bin claude\n  --opencode-bin opencode",
        );
        return Ok(());
    }
    let f = Flags::parse(
        args,
        &["pi-bin", "gemini-bin", "claude-bin", "opencode-bin"],
        &[],
    )?;
    f.only_flags()?;
    for binary in [
        "codex",
        "agy",
        f.get("pi-bin", "pi"),
        f.get("gemini-bin", "gemini"),
        f.get("claude-bin", "claude"),
        f.get("opencode-bin", "opencode"),
        "tailscale",
        "cloudflared",
    ] {
        let Some(path) = find_binary(binary) else {
            println!("{binary}: not installed");
            continue;
        };
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new(&path)
                .arg("--version")
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await;
        if let Ok(Ok(out)) = output
            && out.status.success()
        {
            println!("{binary}: {}", String::from_utf8_lossy(&out.stdout).trim());
            continue;
        }
        println!(
            "{binary}: found at {} (version check failed)",
            path.display()
        );
    }
    println!(
        "Login is not checked. Sign in with each CLI before serving it. For community pi-bob, use Pi /login ibm-bob. Tunnels are optional."
    );
    Ok(())
}
fn share(args: &[String]) -> anyhow::Result<()> {
    if args::help(args) {
        command_help(
            "share",
            "  --via tailscale|funnel|cloudflare\n  --port 8787",
        );
        return Ok(());
    }
    let f = Flags::parse(args, &["via", "port"], &[])?;
    f.only_flags()?;
    let port = f.number("port", "8787", 1, 65535)?;
    match f.get("via", "tailscale") {
        via @ ("tailscale" | "funnel") => {
            println!(
                "tailscale {} http://127.0.0.1:{port}",
                if via == "funnel" { "funnel" } else { "serve" }
            );
            eprintln!(
                "Visibility: {}. Run the command above, then use its HTTPS URL plus /v1. A Bearer API key is still required.",
                if via == "funnel" {
                    "public internet"
                } else {
                    "private tailnet"
                }
            );
        }
        "cloudflare" => println!(
            "Configure a named Cloudflare Tunnel HTTPS hostname pointing to http://127.0.0.1:{port}.\nQuick Tunnels do not support SSE. See docs/remote-access.md."
        ),
        _ => anyhow::bail!("via must be tailscale, funnel, or cloudflare"),
    }
    Ok(())
}
