package harness

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
)

// IBMBob runs the user's explicitly installed community pi-bob extension.
type IBMBob struct{ Binary, Extension string }

func (IBMBob) ID() string { return "ibm-bob" }
func (b IBMBob) Run(ctx context.Context, req Request, emit func(string) error) (*Usage, error) {
	binary := b.Binary
	if binary == "" {
		binary = "pi"
	}
	if !filepath.IsAbs(b.Extension) {
		return nil, errors.New("pi-bob extension must be an absolute local path")
	}
	model := req.Model
	if model == "default" {
		model = "premium"
	}
	args := []string{"--mode", "json", "--no-session", "--no-tools", "--no-extensions", "--no-skills", "--no-prompt-templates", "--no-themes", "--no-context-files", "--extension", b.Extension, "--provider", "ibm-bob", "--model", model}
	env := childEnvironment()
	// CODEX_HOME is not relevant to Pi; keep only common runtime settings and Bob options.
	filtered := env[:0]
	for _, entry := range env {
		if !strings.HasPrefix(entry, "CODEX_HOME=") {
			filtered = append(filtered, entry)
		}
	}
	for _, name := range []string{"PI_CODING_AGENT_DIR", "PI_OFFLINE", "IBM_BOB_API_KEY", "IBM_BOB_KEY", "IBM_BOB_API", "IBM_BOB_BASE_URL", "IBM_BOB_AUTH_BASE_URL", "IBM_BOB_AUTH_SCHEME", "IBM_BOB_MODELS", "IBM_BOB_DISCOVER_MODELS", "IBM_BOB_MODEL_DISCOVERY_TIMEOUT_MS", "IBM_BOB_TOKEN_REQUEST_TIMEOUT_MS", "IBM_BOB_INSTANCE_ID", "IBM_BOB_TEAM_ID", "IBM_BOB_READ_BOBSHELL_SETTINGS", "IBM_BOB_HEADERS_JSON", "IBM_BOB_USER_AGENT", "IBM_BOB_INPUT", "IBM_BOB_REASONING", "IBM_BOB_REASONING_MODELS", "IBM_BOB_CONTEXT_WINDOW", "IBM_BOB_MAX_TOKENS", "IBM_BOB_CREDITS_PER_MILLION", "IBM_BOB_SUPPORTS_DEVELOPER_ROLE", "IBM_BOB_SUPPORTS_REASONING_EFFORT", "IBM_BOB_SUPPORTS_USAGE_IN_STREAMING", "IBM_BOB_SUPPORTS_STRICT_MODE", "IBM_BOB_MAX_TOKENS_FIELD", "IBM_BOB_SUPPORTS_EAGER_TOOL_INPUT_STREAMING", "IBM_BOB_SUPPORTS_LONG_CACHE_RETENTION", "IBM_BOB_SUPPORTS_CACHE_CONTROL_ON_TOOLS", "IBM_BOB_FORCE_ADAPTIVE_THINKING", "IBM_BOB_ALLOW_EMPTY_SIGNATURE"} {
		if value, ok := os.LookupEnv(name); ok {
			filtered = append(filtered, name+"="+value)
		}
	}
	var usage *Usage
	started, done := false, false
	err := runLinesEnv(ctx, binary, args, req.Prompt, filtered, func(line []byte) error {
		var e struct {
			Type    string `json:"type"`
			Message struct {
				Role, Provider, Model, StopReason string
				Usage                             *struct{ Input, Output, CacheRead, CacheWrite, TotalTokens int } `json:"usage"`
			} `json:"message"`
			Delta struct{ Type, Delta string } `json:"assistantMessageEvent"`
		}
		if err := json.Unmarshal(line, &e); err != nil {
			return errors.New("invalid Pi JSON event")
		}
		switch e.Type {
		case "message_start":
			if e.Message.Role == "assistant" {
				if started || done {
					return errors.New("unexpected extra Pi assistant message")
				}
				if e.Message.Provider != "ibm-bob" || e.Message.Model != model {
					return errors.New("Pi selected an unexpected provider or model")
				}
				started = true
			}
		case "message_update":
			if e.Delta.Type == "text_delta" {
				if !started || done {
					return errors.New("unexpected Pi text event")
				}
				return emit(e.Delta.Delta)
			}
		case "message_end":
			if e.Message.Role == "assistant" {
				if !started || done || e.Message.Provider != "ibm-bob" || e.Message.Model != model {
					return errors.New("Pi selected an unexpected provider or model")
				}
				if e.Message.StopReason != "stop" && e.Message.StopReason != "length" {
					return errors.New("Pi Bob failed; check login, model access and configuration")
				}
				if u := e.Message.Usage; u != nil {
					usage = &Usage{PromptTokens: u.Input + u.CacheRead + u.CacheWrite, CompletionTokens: u.Output, TotalTokens: u.TotalTokens}
				}
				done = true
			}
		case "agent_start", "turn_start", "turn_end", "agent_end", "agent_settled", "session":
		case "error", "tool_execution_start":
			return errors.New("Pi failed or attempted unexpected tool execution")
		default: // Ignore non-text progress, but never forward it.
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if !done {
		return nil, errors.New("Pi exited without a completed assistant message")
	}
	return usage, nil
}
