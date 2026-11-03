package harness

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
)

type Antigravity struct{ Binary string }

func (Antigravity) ID() string { return "antigravity" }
func (a Antigravity) Run(ctx context.Context, req Request, emit func(string) error) (*Usage, error) {
	binary := a.Binary
	if binary == "" {
		binary = "agy"
	}
	args := []string{"--input-format", "stream-json", "--output-format", "stream-json", "--sandbox"}
	if req.Model != "default" {
		args = append(args, "--model", req.Model)
	}
	input, err := json.Marshal(map[string]any{"event": "user", "message": map[string]string{"content": req.Prompt}})
	if err != nil {
		return nil, err
	}
	var usage *Usage
	done := false
	var text strings.Builder
	err = runLines(ctx, binary, args, string(input)+"\n", func(line []byte) error {
		var event struct {
			Event string `json:"event"`
			Step  struct {
				Type  string `json:"step_type"`
				Delta string `json:"text_delta"`
			} `json:"step_update"`
			Result struct {
				Status   string `json:"status"`
				Response string `json:"response"`
				Usage    *struct {
					Input  int `json:"input_tokens"`
					Output int `json:"output_tokens"`
					Total  int `json:"total_tokens"`
				} `json:"usage"`
			} `json:"result"`
		}
		if err := json.Unmarshal(line, &event); err != nil {
			return err
		}
		if done {
			return errors.New("Antigravity emitted events after result")
		}
		switch event.Event {
		case "init":
		case "step_update":
			if event.Step.Type == "agent_response" && event.Step.Delta != "" {
				text.WriteString(event.Step.Delta)
				return emit(event.Step.Delta)
			}
		case "result":
			if event.Result.Status != "SUCCESS" {
				return errors.New("Antigravity failed; check login, model access and configuration")
			}
			if !strings.HasPrefix(event.Result.Response, text.String()) {
				return errors.New("Antigravity final response differs from stream")
			}
			if rest := event.Result.Response[text.Len():]; rest != "" {
				if err := emit(rest); err != nil {
					return err
				}
			}
			done = true
			if u := event.Result.Usage; u != nil {
				usage = &Usage{PromptTokens: u.Input, CompletionTokens: u.Output, TotalTokens: u.Total}
			}
		default:
			return errors.New("unknown Antigravity event; check CLI compatibility")
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if !done {
		return nil, errors.New("Antigravity exited without result")
	}
	return usage, nil
}
