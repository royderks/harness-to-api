package harness

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"strings"
)

// OpenCode invokes the OpenCode v2 CLI in standalone mode.
type OpenCode struct{ Binary string }

func (OpenCode) ID() string { return "opencode" }

func (o OpenCode) Run(ctx context.Context, req Request, emit func(string) error) (*Usage, error) {
	binary := o.Binary
	if binary == "" {
		binary = "opencode"
	}
	config, err := os.MkdirTemp("", "h2a-opencode-config-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(config)
	env := make([]string, 0)
	for _, entry := range childEnvironment() {
		if !strings.HasPrefix(entry, "XDG_CONFIG_HOME=") && !strings.HasPrefix(entry, "CODEX_HOME=") {
			env = append(env, entry)
		}
	}
	env = append(env, "XDG_CONFIG_HOME="+config, "OPENCODE_DISABLE_PROJECT_CONFIG=1", "OPENCODE_DISABLE_CLAUDE_CODE=1", `OPENCODE_CONFIG_CONTENT={"plugins":["-*","opencode.*"],"policies":[{"action":"tool.use","resource":"*","effect":"deny"},{"action":"mcp.use","resource":"*","effect":"deny"},{"action":"skill.use","resource":"*","effect":"deny"}],"compaction":{"auto":false}}`)
	args := []string{"run", "--standalone", "--format", "json"}
	if req.Model != "default" {
		args = append(args, "--model", req.Model)
	}
	var usage *Usage
	done := false
	err = runLinesEnv(ctx, binary, args, req.Prompt, env, func(line []byte) error {
		var e struct {
			Type string `json:"type"`
			Part struct {
				Type   string `json:"type"`
				Text   string `json:"text"`
				Reason string `json:"reason"`
				Tokens *struct {
					Input     int `json:"input"`
					Output    int `json:"output"`
					Reasoning int `json:"reasoning"`
					Cache     struct {
						Read  int `json:"read"`
						Write int `json:"write"`
					} `json:"cache"`
				} `json:"tokens"`
			} `json:"part"`
		}
		if err := json.Unmarshal(line, &e); err != nil || e.Type == "" {
			return errors.New("invalid OpenCode JSON event")
		}
		if done {
			return errors.New("OpenCode emitted events after completion")
		}
		switch e.Type {
		case "text":
			if e.Part.Type != "text" {
				return errors.New("invalid OpenCode text event")
			}
			return emit(e.Part.Text)
		case "step_finish":
			if e.Part.Type != "step-finish" || (e.Part.Reason != "stop" && e.Part.Reason != "length") {
				return errors.New("OpenCode did not complete a text response")
			}
			done = true
			if t := e.Part.Tokens; t != nil {
				input := t.Input + t.Cache.Read + t.Cache.Write
				usage = &Usage{PromptTokens: input, CompletionTokens: t.Output + t.Reasoning, TotalTokens: input + t.Output + t.Reasoning}
			}
		case "error", "tool_use":
			return errors.New("OpenCode failed or attempted a tool; check login, model access and configuration")
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if !done {
		return nil, errors.New("OpenCode exited without completing a turn")
	}
	return usage, nil
}
