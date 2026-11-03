# Gemini CLI adapter

The `gemini` adapter uses the locally installed [Gemini CLI](https://github.com/google-gemini/gemini-cli) and its existing official login. Sign in with the CLI separately before starting the gateway; the adapter cannot complete an interactive login. Gemini CLI 0.26.0 is the version whose flags and implementation were checked; older or newer releases have not been verified. The gateway's `gemini/default` model leaves the CLI's model selection unchanged; an explicit model suffix is passed through with `--model`.

The adapter sends the prompt on stdin and reads the CLI's [streaming JSON events](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/headless.md). It forwards assistant text only, returns the terminal token statistics when available, and treats tool events or unsuccessful and incomplete streams as errors. Non-text events and warnings are not forwarded. Gemini's reported total can include token categories beyond input and output, so it need not equal their sum.

Each request runs in a temporary working directory. A temporary system settings override sets the built-in tool allowlist empty, disables hooks, skills and experimental agents, suppresses tool discovery commands, and restricts MCP to a randomly named nonexistent server. `--extensions none` disables installed extensions. A unique context filename prevents ordinary `GEMINI.md` files from being loaded. These controls use the settings and flags available in 0.26.0; they are not an operating-system sandbox, and future CLI releases may change their behavior. User settings and authentication still apply where not overridden. The system settings path is replaced for this subprocess, so existing system-level settings at the normal path are not applied. The subprocess receives a limited environment and does not inherit gateway API keys or other providers' keys. It may still use Gemini's normal login and cache files in the user's home directory.

A follow-up through the built HTTP gateway identified the live blocker: the installed CLI reports that this account requires `GOOGLE_CLOUD_PROJECT` or `GOOGLE_CLOUD_PROJECT_ID`. Neither is configured here. Set the project authorized for your account before serving; see [Google's authentication guide](https://geminicli.com/docs/get-started/authentication/#set-your-google-cloud-project). No successful Gemini inference is claimed. The adapter forwards these project variables and the documented Google/Gemini API-key, Vertex location, and application-credential variables only to Gemini. Gateway keys and other providers' keys remain excluded.

Install [Gemini CLI](https://geminicli.com/docs/get-started/installation/), sign in with `gemini`, then:

```sh
export GOOGLE_CLOUD_PROJECT="YOUR_PROJECT_ID" # only for accounts requiring a project
export OPENAI_API_KEY="$(h2a keys create --name gemini-client --models 'gemini/*')"
h2a serve --provider gemini
```

Use `gemini/default` or `gemini/<model-id>` in requests. `--gemini-bin PATH` selects a non-default executable. The reusable `./demo/run.sh --provider gemini` checks a real request and key revocation after login is ready. Fixtures exercise the protocol without contacting Gemini and are not evidence of live inference.
