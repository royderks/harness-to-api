package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"
)

func main() {
	input, _ := io.ReadAll(os.Stdin)
	args := strings.Join(os.Args[1:], " ")
	cwd, _ := os.Getwd()
	if !strings.HasPrefix(filepath.Base(cwd), "h2a-") || os.Getenv("HARNESS_API_KEY") != "" {
		os.Exit(8)
	}
	mode := "default"
	for i, arg := range os.Args {
		if arg == "--model" && i+1 < len(os.Args) {
			mode = os.Args[i+1]
		}
	}
	if mode == "hang" {
		for {
			time.Sleep(time.Second)
		}
	}
	if mode == "bad-json" {
		fmt.Println("not JSON")
		return
	}
	if mode == "exit-error" {
		fmt.Fprintln(os.Stderr, "sensitive credentials")
		os.Exit(1)
	}
	codex := len(os.Args) > 1 && os.Args[1] == "exec"
	if codex {
		if !strings.Contains(args, "--sandbox read-only") || !strings.Contains(args, "--disable shell_tool") || !strings.HasSuffix(args, " -") {
			os.Exit(9)
		}
		if len(input) == 0 {
			os.Exit(10)
		}
		fmt.Println(`{"type":"thread.started","thread_id":"test"}`)
		fmt.Println(`{"type":"item.completed","item":{"id":"r","type":"reasoning","text":"private reasoning"}}`)
		fmt.Println(`{"type":"item.completed","item":{"id":"a","type":"agent_message","text":"Hello 🌍"}}`)
		if mode == "missing-result" {
			return
		}
		fmt.Println(`{"type":"turn.completed","usage":{"input_tokens":5,"output_tokens":2}}`)
	} else {
		if !strings.Contains(args, "--input-format stream-json --output-format stream-json --sandbox") {
			os.Exit(9)
		}
		var prompt struct {
			Event   string `json:"event"`
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		}
		if json.Unmarshal(input, &prompt) != nil || prompt.Event != "user" || prompt.Message.Content == "" {
			os.Exit(10)
		}
		fmt.Println(`{"event":"init","init":{}}`)
		fmt.Println(`{"event":"step_update","step_update":{"step_type":"tool","text_delta":"secret tool output"}}`)
		fmt.Println(`{"event":"step_update","step_update":{"step_type":"agent_response","state":"ACTIVE","text_delta":"Hello "}}`)
		fmt.Println(`{"event":"step_update","step_update":{"step_type":"agent_response","state":"DONE","text_delta":"🌍"}}`)
		if mode == "missing-result" {
			return
		}
		if mode == "failed" {
			fmt.Println(`{"event":"result","result":{"status":"ERROR","response":""}}`)
			return
		}
		fmt.Println(`{"event":"result","result":{"status":"SUCCESS","response":"Hello 🌍","usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}}`)
	}
}
