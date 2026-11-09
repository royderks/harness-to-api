# OpenCode adapter

The adapter targets OpenCode CLI v2 (`@opencode/cli`), using `run --standalone --format json`. Install the official CLI separately and authenticate with `opencode auth login`; the gateway never reads or exports credentials. Select a model as `opencode/provider/model` (the entire `provider/model` suffix is passed as one `--model` argument), or `opencode/default` for OpenCode's default. The default may be unavailable when the user's global configuration is isolated, so an explicit model is recommended.

Each request runs in a temporary working directory with stdin as its prompt. It uses a temporary XDG config directory, disables project configuration, disables Claude Code discovery, turns off non-`opencode.*` plugins, denies tool, MCP and skill use by policy, and disables automatic compaction. The gateway forwards only the shared child environment allowlist, not provider API key environment variables or the gateway's `OPENAI_API_KEY`; use the OpenCode auth store. `opencode.*` built-in plugins remain enabled because v2 needs them for its agent and provider functionality. System or managed OpenCode configuration and built-in plugins can still apply; this is not an OS sandbox or a guarantee against administrator-controlled extensions, instructions, or networking. The temporary XDG config also hides custom providers, configured defaults, and global instructions. OpenCode retains its own session data in its normal data directory; standalone mode does not mean ephemeral session storage.

Only text parts are emitted. A `step_finish` with reason `stop` or `length` is required for success; error or tool events, malformed JSON, nonterminal finishes, and streams missing that terminal event fail. Usage counts include cache reads/writes in input and reasoning in output when reported; absent usage remains unknown. CLI releases that omit the terminal event may therefore fail even after producing text. The process is cancelled on request cancellation or a downstream emit error. This is the existing text adapter contract, not an OpenCode server or OpenAI API proxy.

References: [v2 CLI commands](https://dev.opencode.ai/v2/docs/cli/commands/), [configuration and precedence](https://dev.opencode.ai/v2/docs/config/), [plugins](https://dev.opencode.ai/v2/docs/plugins/), [policies](https://dev.opencode.ai/v2/docs/policies/), and [CLI event implementation](https://github.com/anomalyco/opencode/blob/dev/packages/cli/src/run/noninteractive.ts).

The adapter protocol is tested with OpenCode 2.2.5. Fixtures do not establish live account access.

```sh
npm install -g @opencode/cli@2.2.5
opencode auth login
export OPENAI_API_KEY="$(h2a keys create --name opencode-client --models 'opencode/*')"
h2a serve --provider opencode
```

Pass an available model as `opencode/PROVIDER/MODEL`, or try the harness's default with `opencode/default`. Use `--opencode-bin PATH` for a non-default executable. This adapter targets v2; the older `opencode-ai` CLI is not the tested runtime.
