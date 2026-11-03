use super::{Flags, args, providers, split_list, state};
use crate::{api, auth};
use serde_json::json;

pub fn run(args: &[String]) -> anyhow::Result<()> {
    if args.is_empty() || ["help", "--help", "-h"].contains(&args[0].as_str()) {
        eprintln!(
            "Usage: h2a keys <create|list|revoke|inspect|explain> [flags]\n\nCreate prints a secret once; inspect and explain show read-only snapshots. Flags precede KEY_ID."
        );
        return Ok(());
    }
    let command = args[0].as_str();
    anyhow::ensure!(
        ["create", "list", "revoke", "inspect", "explain"].contains(&command),
        "unknown keys command"
    );
    if args::help(&args[1..]) {
        super::command_help(
            &format!("keys {command}"),
            "  --state-dir PATH\n  --name default\n  --models '*'\n  --ttl 1h\n  --expires-at RFC3339\n  --request-timeout 90s\n  --max-requests 20\n  --rate-limit 5/min\n  --max-concurrent 1\n  --validate\n  --provider codex\n  --allowed-models provider/model,...\n  --model provider/model (explain)",
        );
        return Ok(());
    }
    let mut strings = vec!["state-dir"];
    if command == "create" {
        strings.extend([
            "name",
            "models",
            "ttl",
            "expires-at",
            "request-timeout",
            "max-requests",
            "rate-limit",
            "max-concurrent",
        ]);
    }
    if command == "create" || command == "explain" {
        strings.extend(["provider", "allowed-models"]);
    }
    if command == "explain" {
        strings.push("model");
    }
    let f = Flags::parse(
        &args[1..],
        &strings,
        if command == "create" {
            &["validate"]
        } else {
            &[]
        },
    )?;
    if command == "create" || command == "list" {
        f.only_flags()?
    } else {
        anyhow::ensure!(
            f.rest.len() == 1,
            "usage: h2a keys {command} [flags] KEY_ID"
        )
    }
    let store = auth::Store::new(state(&f));
    match command {
        "create" => {
            let policy = policy(&f)?;
            let models = split_list(f.get("models", "*"));
            if f.boolean("validate") || f.seen("provider") || f.seen("allowed-models") {
                let providers = providers(f.get("provider", "codex"))?
                    .iter()
                    .map(|p| p.id().into())
                    .collect::<Vec<String>>();
                let allowed = split_list(f.get("allowed-models", ""));
                api::validate_models(&allowed, &providers)?;
                if f.boolean("validate") {
                    anyhow::ensure!(!allowed.is_empty(), "--validate requires --allowed-models");
                    for model in &models {
                        api::validate_models(std::slice::from_ref(model), &providers)?;
                        api::check_model(model, &models, &providers, &allowed)?;
                    }
                }
            }
            let key = store.create(auth::KeySpec {
                name: f.get("name", "default").into(),
                models,
                policy,
            })?;
            println!("{}", key.token);
            eprintln!(
                "Created key {}. Save stdout securely; this key will not be shown again.",
                key.principal.id
            );
        }
        "list" => println!("{}", serde_json::to_string(&store.list()?)?),
        "revoke" => {
            store.revoke(&f.rest[0])?;
            eprintln!(
                "Key revoked; new requests will be rejected and active runs will be canceled when the server observes revocation."
            );
        }
        "inspect" => println!("{}", serde_json::to_string(&store.inspect(&f.rest[0])?)?),
        "explain" => {
            let providers = providers(f.get("provider", "codex"))?
                .iter()
                .map(|p| p.id().into())
                .collect::<Vec<String>>();
            let allowed = split_list(f.get("allowed-models", ""));
            api::validate_models(&allowed, &providers)?;
            let key = store.inspect(&f.rest[0])?;
            let mut result = json!({"key":key,"account_access":"unknown","note":"Local snapshot using the supplied configuration, not the running server. No live concurrency or upstream account access is checked.","configured_providers":providers,"denials":[]});
            if !allowed.is_empty() {
                result["configured_models"] = json!(allowed);
            }
            let model = f.get("model", "");
            if !model.is_empty() {
                let mut denials = Vec::new();
                let check = api::check_model(model, &key.principal.models, &providers, &allowed);
                result["model"] = json!(model);
                result["model_allowed"] = json!(check.is_ok());
                if let Err(e) = check {
                    result["model_reason"] = json!(e.message);
                    denials.push(e.message);
                }
                if key.expired {
                    denials.push("key expired");
                }
                if !key.principal.can("completions:create") {
                    denials.push("completion permission missing");
                }
                if key.remaining_requests == Some(0) {
                    denials.push("lifetime request quota exhausted");
                }
                if key.principal.policy.requests_per_minute > 0
                    && key.recent_requests >= key.principal.policy.requests_per_minute
                {
                    denials.push("request rate limit reached");
                }
                result["local_policy_allowed"] = json!(denials.is_empty());
                result["denials"] = json!(denials);
            }
            println!("{result}");
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn policy(f: &Flags) -> anyhow::Result<auth::Policy> {
    let mut p = auth::Policy::default();
    anyhow::ensure!(
        !(f.seen("ttl") && f.seen("expires-at")),
        "--ttl and --expires-at are mutually exclusive"
    );
    if f.seen("ttl") {
        let d = auth::DurationValue::parse(f.get("ttl", ""))?;
        anyhow::ensure!(d.0 > 0, "--ttl must be a positive duration");
        p.expires_at = Some(chrono::Utc::now() + chrono::Duration::nanoseconds(d.0));
    }
    if f.seen("expires-at") {
        let t = chrono::DateTime::parse_from_rfc3339(f.get("expires-at", ""))?
            .with_timezone(&chrono::Utc);
        anyhow::ensure!(
            t > chrono::Utc::now(),
            "--expires-at must be a future RFC3339 timestamp"
        );
        p.expires_at = Some(t);
    }
    if f.seen("request-timeout") {
        let d = auth::DurationValue::parse(f.get("request-timeout", ""))?;
        anyhow::ensure!(
            d.0 > 0 && d.0 <= 3_600_000_000_000,
            "--request-timeout must be positive and at most 1h"
        );
        p.request_timeout = d;
    }
    if f.seen("max-requests") {
        p.max_requests = f.number("max-requests", "", 1, u64::MAX)?;
    }
    if f.seen("rate-limit") {
        let raw = f.get("rate-limit", "");
        let n = raw
            .strip_suffix("/min")
            .ok_or_else(|| anyhow::anyhow!("--rate-limit must be N/min"))?
            .parse::<usize>()?;
        anyhow::ensure!(
            (1..=100000).contains(&n),
            "--rate-limit must be 1/min through 100000/min"
        );
        p.requests_per_minute = n;
    }
    if f.seen("max-concurrent") {
        p.max_concurrent = f.number("max-concurrent", "", 1, 128)? as usize;
    }
    Ok(p)
}
