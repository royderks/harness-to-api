package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"
	"time"
)

func main() {
	input, _ := io.ReadAll(os.Stdin)
	args := strings.Join(os.Args[1:], " ")
	if os.Getenv("GOOGLE_CLOUD_PROJECT") != "fixture-project" || os.Getenv("GEMINI_API_KEY") != "fixture-key" || !strings.Contains(args, "--output-format stream-json") || !strings.Contains(args, "--extensions none") || !strings.Contains(args, "--approval-mode default") || os.Getenv("CODEX_HOME") != "" || os.Getenv("OPENAI_API_KEY") != "" {
		os.Exit(2)
	}
	b, err := os.ReadFile(os.Getenv("GEMINI_CLI_SYSTEM_SETTINGS_PATH"))
	if err != nil {
		os.Exit(3)
	}
	var s struct {
		Tools struct {
			Core        []string `json:"core"`
			EnableHooks bool     `json:"enableHooks"`
		} `json:"tools"`
		MCP struct {
			Allowed []string `json:"allowed"`
		} `json:"mcp"`
		Experimental struct {
			EnableAgents bool `json:"enableAgents"`
			Codebase     struct {
				Enabled bool `json:"enabled"`
			} `json:"codebaseInvestigatorSettings"`
			Help struct {
				Enabled bool `json:"enabled"`
			} `json:"cliHelpAgentSettings"`
		} `json:"experimental"`
		Hooks struct {
			Enabled bool `json:"enabled"`
		} `json:"hooksConfig"`
	}
	if json.Unmarshal(b, &s) != nil || s.Tools.Core == nil || len(s.Tools.Core) != 0 || s.Tools.EnableHooks || len(s.MCP.Allowed) != 1 || s.Experimental.EnableAgents || s.Experimental.Codebase.Enabled || s.Experimental.Help.Enabled || s.Hooks.Enabled {
		os.Exit(4)
	}
	switch string(input) {
	case "valid":
		if strings.Contains(args, "--model") {
			os.Exit(5)
		}
		fmt.Println(`{"type":"init"}`)
		fmt.Println(`{"type":"message","role":"user","content":"private"}`)
		fmt.Println(`{"type":"message","role":"assistant","content":{"image":"private"}}`)
		fmt.Println(`{"type":"warning","content":"private"}`)
		fmt.Println(`{"type":"error","severity":"warning","message":"private"}`)
		fmt.Println(`{"type":"message","role":"assistant","content":"hello ","delta":true}`)
		fmt.Println(`{"type":"message","role":"assistant","content":"world","delta":true}`)
		fmt.Println(`{"type":"result","status":"success","stats":{"input_tokens":12,"output_tokens":3,"total_tokens":16}}`)
	case "model":
		if !strings.Contains(args, "--model gemini-test") {
			os.Exit(6)
		}
		fmt.Println(`{"type":"result","status":"success"}`)
	case "error":
		fmt.Println(`{"type":"result","status":"error"}`)
	case "error-event":
		fmt.Println(`{"type":"error","severity":"error"}`)
	case "incomplete":
		fmt.Println(`{"type":"init"}`)
	case "malformed":
		fmt.Println(`{broken`)
	case "tool":
		fmt.Println(`{"type":"tool_use","tool_name":"run_shell_command"}`)
	case "cancel":
		time.Sleep(30 * time.Second)
	case "emit":
		fmt.Println(`{"type":"message","role":"assistant","content":"text"}`)
		time.Sleep(30 * time.Second)
	}
}
