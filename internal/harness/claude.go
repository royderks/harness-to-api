package harness

import (
	"context"
	"encoding/json"
	"errors"
	"os"
)

type Claude struct{ Binary string }

func (Claude) ID() string { return "claude" }

func (c Claude) Run(ctx context.Context, req Request, emit func(string) error) (*Usage, error) {
	binary := c.Binary
	if binary == "" {
		binary = "claude"
	}
	args := []string{"--print", "--safe-mode", "--setting-sources", "", "--settings", `{"disableAllHooks":true}`, "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--tools", "", "--disallowedTools", "*", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence", "--no-chrome"}
	if req.Model != "" && req.Model != "default" {
		args = append(args, "--model", req.Model)
	}
	env := childEnvironment()
	for _, name := range []string{"ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"} {
		if value, ok := os.LookupEnv(name); ok {
			env = append(env, name+"="+value)
		}
	}
	var usage *Usage
	done, partial := false, false
	err := runLinesEnv(ctx, binary, args, req.Prompt, env, func(line []byte) error {
		var e struct {
			Type       string  `json:"type"`
			Subtype    string  `json:"subtype"`
			IsError    *bool   `json:"is_error"`
			Error      string  `json:"error"`
			IsAPIError bool    `json:"is_api_error_message"`
			Parent     *string `json:"parent_tool_use_id"`
			Event      struct {
				Type  string `json:"type"`
				Delta struct {
					Type string `json:"type"`
					Text string `json:"text"`
				} `json:"delta"`
			} `json:"event"`
			Message struct {
				Content []struct {
					Type string `json:"type"`
					Text string `json:"text"`
				} `json:"content"`
			} `json:"message"`
			Usage *struct {
				Input    int `json:"input_tokens"`
				Output   int `json:"output_tokens"`
				Read     int `json:"cache_read_input_tokens"`
				Creation int `json:"cache_creation_input_tokens"`
			} `json:"usage"`
		}
		if err := json.Unmarshal(line, &e); err != nil || e.Type == "" {
			return errors.New("invalid Claude JSON event")
		}
		if done {
			return errors.New("Claude emitted events after completion")
		}
		switch e.Type {
		case "stream_event":
			if e.Parent == nil && e.Event.Type == "content_block_delta" && e.Event.Delta.Type == "text_delta" {
				partial = true
				return emit(e.Event.Delta.Text)
			}
		case "assistant":
			if e.Error != "" || e.IsAPIError {
				return errors.New("Claude failed; check authentication, model access and configuration")
			}
			if e.Parent != nil {
				return nil
			}
			for _, block := range e.Message.Content {
				if block.Type == "tool_use" || block.Type == "server_tool_use" {
					return errors.New("Claude attempted a tool call")
				}
				if block.Type == "text" && !partial {
					if err := emit(block.Text); err != nil {
						return err
					}
				}
			}
			partial = false
		case "result":
			if e.Subtype != "success" || e.IsError == nil || *e.IsError {
				return errors.New("Claude failed; check authentication, model access and configuration")
			}
			done = true
			if u := e.Usage; u != nil {
				input := u.Input + u.Read + u.Creation
				usage = &Usage{PromptTokens: input, CompletionTokens: u.Output, TotalTokens: input + u.Output}
			}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if !done {
		return nil, errors.New("Claude exited without a successful result")
	}
	return usage, nil
}
