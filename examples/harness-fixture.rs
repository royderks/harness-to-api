use serde_json::json;
use std::{
    io::{self, Read},
    time::Duration,
};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    if !std::env::current_dir()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("h2a-")
        || std::env::var_os("HARNESS_API_KEY").is_some()
        || std::env::var_os("OPENAI_API_KEY").is_some()
    {
        std::process::exit(8)
    }
    let mode = args
        .windows(2)
        .find(|w| w[0] == "--model")
        .map(|w| w[1].as_str())
        .unwrap_or("default");
    if mode == "hang" {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if mode == "drop-guard" || mode == "emit-hang" {
        println!(
            "{}",
            json!({"type":"item.completed","item":{"type":"agent_message","text":format!("{}|{}", std::env::current_dir().unwrap().display(), std::process::id())}})
        );
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if mode == "bad-json" {
        println!("not JSON");
        return;
    }
    if mode == "exit-error" {
        std::process::exit(1)
    }
    let (provider, mut expected): (&str, Vec<&str>) = if args.first().is_some_and(|x| x == "exec") {
        (
            "codex",
            vec![
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
            ],
        )
    } else if args.first().is_some_and(|x| x == "--input-format") {
        (
            "agy",
            vec![
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--sandbox",
            ],
        )
    } else if args.first().is_some_and(|x| x == "--mode") {
        (
            "bob",
            vec![
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
                args.get(10).unwrap(),
                "--provider",
                "ibm-bob",
            ],
        )
    } else if args.first().is_some_and(|x| x == "--output-format") {
        (
            "gemini",
            vec![
                "--output-format",
                "stream-json",
                "--extensions",
                "none",
                "--approval-mode",
                "default",
            ],
        )
    } else if args.first().is_some_and(|x| x == "--print") {
        (
            "claude",
            vec![
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
            ],
        )
    } else {
        ("opencode", vec!["run", "--standalone", "--format", "json"])
    };
    if provider == "bob" || mode != "default" {
        expected.extend(["--model", mode]);
    }
    if provider == "codex" {
        expected.push("-");
    }
    if args != expected || (provider != "agy" && input.is_empty()) {
        std::process::exit(9)
    }
    if provider == "agy"
        && serde_json::from_str::<serde_json::Value>(&input)
            .ok()
            .and_then(|v| v["message"]["content"].as_str().map(str::to_owned))
            .unwrap_or_default()
            .is_empty()
    {
        std::process::exit(9)
    }
    if provider == "gemini"
        && !std::path::Path::new(
            &std::env::var("GEMINI_CLI_SYSTEM_SETTINGS_PATH").unwrap_or_default(),
        )
        .is_file()
    {
        std::process::exit(10)
    }
    if provider == "opencode"
        && (std::env::var("OPENCODE_DISABLE_PROJECT_CONFIG").as_deref() != Ok("1")
            || std::env::var("OPENCODE_DISABLE_CLAUDE_CODE").as_deref() != Ok("1"))
    {
        std::process::exit(10)
    }
    if matches!(provider, "bob" | "gemini" | "opencode") && std::env::var_os("CODEX_HOME").is_some()
    {
        std::process::exit(10)
    }
    let events = match provider {
        "codex" => vec![
            json!({"type":"item.completed","item":{"type":"agent_message","text":"Hello 🌍"}}),
            json!({"type":"turn.completed","usage":{"input_tokens":5,"output_tokens":2}}),
        ],
        "agy" => vec![
            json!({"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"Hello 🌍"}}),
            json!({"event":"result","result":{"status":if mode=="failed" {"ERROR"} else {"SUCCESS"},"response":"Hello 🌍","usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}}),
        ],
        "bob" => vec![
            json!({"type":"message_start","message":{"role":"assistant","provider":if mode=="wrong-provider" {"other"} else {"ibm-bob"},"model":mode}}),
            json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Hello 🌍"}}),
            json!({"type":"message_end","message":{"role":"assistant","provider":"ibm-bob","model":mode,"stopReason":if mode=="failed" {"error"} else {"stop"},"usage":{"input":3,"output":2,"cacheRead":2,"totalTokens":7}}}),
        ],
        "gemini" => vec![
            json!({"type":"message","role":"assistant","content":"Hello 🌍"}),
            json!({"type":"result","status":"success","stats":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}),
        ],
        "claude" => vec![
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"Hello 🌍"}]}}),
            json!({"type":"result","subtype":"success","is_error":false,"usage":{"input_tokens":5,"output_tokens":2}}),
        ],
        _ => vec![
            json!({"type":"text","part":{"type":"text","text":"Hello 🌍"}}),
            json!({"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":5,"output":2}}}),
        ],
    };
    if mode == "tool-event" {
        println!(
            "{}",
            json!({"type":if provider=="bob" {"tool_execution_start"} else {"tool_use"}})
        );
        return;
    }
    let count = if mode == "missing-result" {
        events.len() - 1
    } else {
        events.len()
    };
    for e in events.into_iter().take(count) {
        println!("{e}");
    }
}
