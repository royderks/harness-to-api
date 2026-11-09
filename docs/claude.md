# Claude Code adapter

The `claude` adapter runs the official Claude Code CLI in noninteractive `stream-json` mode, with an isolated temporary working directory and a fresh, non-persistent session per request. It forwards the requested model unchanged to `--model` (except `default`, which uses Claude's default). Claude Code resolves aliases. Text deltas are forwarded; thinking, tool, subagent and other non-text events are not. A successful terminal result and process exit are required. Usage includes uncached input, cache creation and cache read tokens, plus output tokens when Claude reports usage.

The adapter uses `--safe-mode`, an empty `--setting-sources`, and `disableAllHooks` in explicit settings. It disables built-in tools, MCP tools, slash commands and Chrome. Safe mode suppresses user customizations including hooks, plugins, skills, memory and CLAUDE.md; managed policy remains in effect and can include policy hooks or settings that take precedence. Built-in components still run. This is not an operating-system sandbox. The adapter does not send local files as context, but CLI-generated system context and service-side behavior still apply. The shared runner bounds output and handles cancellation. Tool use, multimodal input, session continuation and structured output are unsupported.

Safe mode preserves the CLI's normal authentication, including an existing subscription login. The adapter also forwards `ANTHROPIC_API_KEY` and `CLAUDE_CODE_OAUTH_TOKEN` when set, through official CLI environment interfaces; it does not pass gateway `OPENAI_API_KEY`, extract credentials, or call private endpoints. It intentionally does not use `--bare`, which excludes stored OAuth and keychain credentials. See the [CLI reference](https://code.claude.com/docs/en/cli-reference) and [programmatic use](https://code.claude.com/docs/en/headless). Use Claude Code 2.1.317 or later; this is the minimum version tested here, not a claim about the first version supporting these flags. The official 2.1.317 CLI accepted these flags and reported no tools or MCP servers during initialization. Protocol checks do not establish live account access.

Install [Claude Code](https://code.claude.com/docs/en/setup), run `claude auth login`, then:

```sh
export OPENAI_API_KEY="$(h2a keys create --name claude-client --models 'claude/*')"
h2a serve --provider claude
```

Use `claude/default`, `claude/sonnet`, or another model ID accepted by your CLI/account. `--claude-bin PATH` selects a non-default executable. `./demo/run.sh --provider claude` runs the live demo once your account is configured.
