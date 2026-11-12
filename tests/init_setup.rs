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

#[test]
fn installed_pi_with_invalid_extension_gets_extension_specific_guidance() {
    check_pi_extension_guidance(if cfg!(windows) { "pi.exe" } else { "pi" }, ".EXE");
}

#[cfg(windows)]
#[test]
fn pi_cmd_shim_is_detected_before_extension_validation() {
    check_pi_extension_guidance("pi.cmd", ".CMD;.EXE");
}

fn check_pi_extension_guidance(binary: &str, pathext: &str) {
    let temp = tempfile::tempdir().unwrap();
    let pi = temp.path().join(binary);
    // The executable is only resolved: extension validation precedes invocation.
    std::fs::copy(env!("CARGO_BIN_EXE_h2a"), pi).unwrap();
    let state = temp.path().join("state");
    let empty_extension = temp.path().join("empty-extension");
    std::fs::create_dir(&empty_extension).unwrap();
    for extension in ["", "relative/pi-bob", empty_extension.to_str().unwrap()] {
        let output = Command::new(env!("CARGO_BIN_EXE_h2a"))
            .args([
                "init",
                "--provider",
                "ibm-bob",
                "--model",
                "ibm-bob/default",
                "--state-dir",
                state.to_str().unwrap(),
                "--port",
                "8787",
                "--pi-bob-extension",
                extension,
                "--no-probe",
                "--json",
            ])
            .env("PATH", temp.path())
            .env("PATHEXT", pathext)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        let event: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(event["code"], "bob_extension_required");
        let next = event["next_step"].as_str().unwrap();
        assert!(next.contains("extensions/bob.ts"));
        assert!(next.contains("h2a init --provider ibm-bob --pi-bob-extension"));
        assert!(!next.contains("pi-coding-agent"));
        assert!(!state.exists());
    }
}
