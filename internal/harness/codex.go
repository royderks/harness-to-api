package harness

import (
	"context"
	"encoding/json"
	"errors"
)

type Codex struct{ Binary string }

func (Codex) ID() string { return "codex" }
func (c Codex) Run(ctx context.Context, req Request, emit func(string) error) (*Usage, error) {
	binary := c.Binary
	if binary == "" {
		binary = "codex"
	}
	args := []string{"exec", "--json", "--ephemeral", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules", "--sandbox", "read-only", "--disable", "shell_tool", "--disable", "hooks", "-c", "approval_policy=\"never\"", "-c", "web_search=\"disabled\""}
	if req.Model != "default" {
		args = append(args, "--model", req.Model)
	}
	args = append(args, "-")
	var usage *Usage
	done := false
	err := runLines(ctx, binary, args, req.Prompt, func(line []byte) error {
		var event struct {
			Type string `json:"type"`
			Item struct {
				Type string `json:"type"`
				Text string `json:"text"`
			} `json:"item"`
			Usage struct {
				Input  int `json:"input_tokens"`
				Output int `json:"output_tokens"`
			} `json:"usage"`
		}
		if err := json.Unmarshal(line, &event); err != nil {
			return err
		}
		if done {
			return errors.New("Codex emitted events after completion")
		}
		switch event.Type {
		case "item.completed":
			if event.Item.Type == "agent_message" {
				return emit(event.Item.Text)
			}
		case "turn.completed":
			done = true
			usage = &Usage{PromptTokens: event.Usage.Input, CompletionTokens: event.Usage.Output, TotalTokens: event.Usage.Input + event.Usage.Output}
		case "turn.failed", "error":
			return errors.New("Codex failed; check login, model access and configuration")
		case "thread.started", "turn.started", "item.started", "item.updated":
		default:
			return errors.New("unknown Codex event; check CLI compatibility")
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if !done {
		return nil, errors.New("Codex exited without completing a turn")
	}
	return usage, nil
}
