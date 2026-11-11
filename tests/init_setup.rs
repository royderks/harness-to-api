use std::process::Command;

#[test]
fn missing_selected_harness_returns_actionable_json_without_state_changes() {
    for (provider, install_hint) in [
        ("codex", "chatgpt.com/codex/install"),
        ("antigravity", "antigravity.google/cli/install"),
        ("claude", "claude.ai/install"),
        ("gemini", "npm install -g @google/gemini-cli"),
        ("opencode", "opencode.ai"),
        ("ibm-bob", "pi-bob@0.2.6"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let output = Command::new(env!("CARGO_BIN_EXE_h2a"))
            .args([
                "init",
                "--provider",
                provider,
                "--model",
                &format!("{provider}/default"),
                "--state-dir",
                state.to_str().unwrap(),
                "--port",
                "8787",
                "--no-probe",
                "--json",
            ])
            .env("PATH", temp.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        let event: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(event["event"], "error");
        assert_eq!(event["code"], "harness_missing");
        let next = event["next_step"].as_str().unwrap();
        assert!(next.contains(install_hint), "{provider}: {next}");
        assert!(next.contains(&format!("h2a init --provider {provider}")));
        assert!(output.stderr.is_empty());
        assert!(!state.exists());
    }
}
