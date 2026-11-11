use crate::harness::Provider;

pub fn name(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "Codex",
        Provider::Antigravity => "Antigravity",
        Provider::IbmBob => "IBM Bob through Pi (community extension)",
        Provider::Gemini => "Gemini CLI",
        Provider::Claude => "Claude Code",
        Provider::OpenCode => "OpenCode 2",
    }
}

pub fn choice(value: &str) -> Option<Provider> {
    value
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|n| Provider::ALL.get(n).copied())
        .or_else(|| {
            Provider::ALL.into_iter().find(|&provider| {
                provider.id().eq_ignore_ascii_case(value)
                    || name(provider).eq_ignore_ascii_case(value)
            })
        })
}

pub fn instructions(provider: Provider, windows: bool) -> String {
    let (install, login, note, docs) = match provider {
        Provider::Codex => (
            if windows {
                "irm https://chatgpt.com/codex/install.ps1 | iex"
            } else {
                "curl -fsSL https://chatgpt.com/codex/install.sh | sh"
            },
            "codex login",
            "",
            "https://learn.chatgpt.com/docs/codex/cli",
        ),
        Provider::Antigravity => (
            if windows {
                "irm https://antigravity.google/cli/install.ps1 | iex"
            } else {
                "curl -fsSL https://antigravity.google/cli/install.sh | bash"
            },
            "agy",
            "",
            "https://antigravity.google/docs/cli/tutorial",
        ),
        Provider::Claude => (
            if windows {
                "irm https://claude.ai/install.ps1 | iex"
            } else {
                "curl -fsSL https://claude.ai/install.sh | bash"
            },
            "claude",
            "",
            "https://code.claude.com/docs/en/quickstart",
        ),
        Provider::Gemini => (
            "npm install -g @google/gemini-cli",
            "gemini",
            "Requires Node.js and npm.\n",
            "https://geminicli.com/docs/get-started/installation/",
        ),
        Provider::OpenCode => (
            if windows {
                "Start-Process 'https://dev.opencode.ai/docs/'"
            } else {
                "curl -fsSL https://opencode.ai/v2/install | bash"
            },
            "opencode auth login",
            if windows {
                "OpenCode provides a manual Windows download: open the page below, select the Windows CLI for your architecture, and place opencode.exe on PATH.\n"
            } else {
                ""
            },
            "https://dev.opencode.ai/docs/",
        ),
        Provider::IbmBob => (
            "npm install -g @earendil-works/pi-coding-agent@1.5.0 pi-bob@0.2.6",
            if windows {
                "pi -e \"$(npm root -g)/pi-bob\""
            } else {
                "PI_OFFLINE=1 pi -e \"$(npm root -g)/pi-bob\""
            },
            "Requires Node.js 22.19+ and npm. Uses the community pi-bob extension; in Pi, run /login ibm-bob. For the extension directory prompt in h2a, use the absolute path printed by npm root -g with /pi-bob appended.\n",
            "https://pi.dev/packages/pi-bob",
        ),
    };
    let retry = if provider == Provider::IbmBob {
        "h2a init --provider ibm-bob --pi-bob-extension \"$(npm root -g)/pi-bob\"".to_string()
    } else {
        format!("h2a init --provider {}", provider.id())
    };
    format!(
        "Add {}:\n{note}\n  {install}\n\nThen sign in:\n\n  {login}\n\nOpen a new terminal if the installer changes PATH, then rerun:\n\n  {retry}\n\nSetup guide: {docs}",
        name(provider),
    )
}

pub fn bob_extension_instructions() -> &'static str {
    "Pi is installed. Set --pi-bob-extension to an absolute pi-bob package directory containing extensions/bob.ts.

Install the community extension if needed:

  npm install -g pi-bob@0.2.6

Then retry interactive setup:

  h2a init --provider ibm-bob --pi-bob-extension \"$(npm root -g)/pi-bob\"

For noninteractive setup, correct --pi-bob-extension and keep your other required flags."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_and_guidance_cover_every_provider() {
        for (index, provider) in Provider::ALL.into_iter().enumerate() {
            assert_eq!(choice(&(index + 1).to_string()), Some(provider));
            assert_eq!(choice(provider.id()), Some(provider));
            assert_eq!(choice(name(provider)), Some(provider));
            assert_eq!(choice(&name(provider).to_uppercase()), Some(provider));
            for windows in [false, true] {
                let text = instructions(provider, windows);
                assert!(text.contains(&format!("h2a init --provider {}", provider.id())));
                assert!(text.contains("https://"));
                if matches!(
                    provider,
                    Provider::Codex | Provider::Antigravity | Provider::Claude
                ) {
                    assert_eq!(text.contains("install.ps1"), windows);
                    assert_eq!(text.contains("install.sh"), !windows);
                }
            }
        }
        for bad in [
            "0",
            "7",
            "all",
            "codex,claude",
            "-1",
            "99999999999999999999999",
            "",
        ] {
            assert_eq!(choice(bad), None);
        }
    }
}
