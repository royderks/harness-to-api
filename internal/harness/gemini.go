package harness

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
)

type Gemini struct{ Binary string }

func (Gemini) ID() string { return "gemini" }

func (g Gemini) Run(ctx context.Context, req Request, emit func(string) error) (*Usage, error) {
	binary := g.Binary
	if binary == "" {
		binary = "gemini"
	}
	dir, err := os.MkdirTemp("", "h2a-gemini-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(dir)
	settings := map[string]any{
		"tools":        map[string]any{"core": []string{}, "enableHooks": false, "discoveryCommand": "", "callCommand": ""},
		"hooksConfig":  map[string]any{"enabled": false},
		"mcp":          map[string]any{"allowed": []string{filepath.Base(dir) + "-no-server"}, "serverCommand": ""},
		"experimental": map[string]any{"enableAgents": false, "codebaseInvestigatorSettings": map[string]bool{"enabled": false}, "cliHelpAgentSettings": map[string]bool{"enabled": false}},
		"skills":       map[string]bool{"enabled": false},
		"context":      map[string]any{"fileName": filepath.Base(dir) + "-no-context", "loadMemoryFromIncludeDirectories": false},
	}
	data, err := json.Marshal(settings)
	if err != nil {
		return nil, err
	}
	settingsPath := filepath.Join(dir, "settings.json")
	if err = os.WriteFile(settingsPath, data, 0600); err != nil {
		return nil, err
	}
	env := childEnvironment()
	filtered := env[:0]
	for _, entry := range env {
		if !strings.HasPrefix(entry, "CODEX_HOME=") {
			filtered = append(filtered, entry)
		}
	}
	env = append(filtered, "GEMINI_CLI_SYSTEM_SETTINGS_PATH="+settingsPath)
	for _, name := range []string{"GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_CLOUD_PROJECT", "GOOGLE_CLOUD_PROJECT_ID", "GOOGLE_CLOUD_LOCATION", "GOOGLE_GENAI_USE_VERTEXAI", "GOOGLE_APPLICATION_CREDENTIALS"} {
		if value, ok := os.LookupEnv(name); ok {
			env = append(env, name+"="+value)
		}
	}
	args := []string{"--output-format", "stream-json", "--extensions", "none", "--approval-mode", "default"}
	if req.Model != "" && req.Model != "default" {
		args = append(args, "--model", req.Model)
	}
	var usage *Usage
	done := false
	err = runLinesEnv(ctx, binary, args, req.Prompt, env, func(line []byte) error {
		var event struct {
			Type     string          `json:"type"`
			Role     string          `json:"role"`
			Content  json.RawMessage `json:"content"`
			Severity string          `json:"severity"`
			Status   string          `json:"status"`
			Stats    *struct {
				Input  int `json:"input_tokens"`
				Output int `json:"output_tokens"`
				Total  int `json:"total_tokens"`
			} `json:"stats"`
		}
		if json.Unmarshal(line, &event) != nil || event.Type == "" {
			return errors.New("invalid Gemini JSON event")
		}
		if done {
			return errors.New("Gemini emitted events after completion")
		}
		switch event.Type {
		case "message":
			if event.Role == "assistant" {
				var content string
				if json.Unmarshal(event.Content, &content) == nil && content != "" {
					return emit(content)
				}
			}
		case "result":
			if event.Status != "success" {
				return errors.New("Gemini failed; check login, model access and configuration")
			}
			done = true
			if s := event.Stats; s != nil {
				usage = &Usage{PromptTokens: s.Input, CompletionTokens: s.Output, TotalTokens: s.Total}
			}
		case "error":
			if event.Severity != "warning" {
				return errors.New("Gemini reported an error; check login, model access and configuration")
			}
		case "tool_use", "tool_result":
			return errors.New("Gemini attempted unexpected tool execution")
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if !done {
		return nil, errors.New("Gemini exited without completing a turn")
	}
	return usage, nil
}
