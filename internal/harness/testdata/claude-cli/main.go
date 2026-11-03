package main

import (
	"fmt"
	"io"
	"os"
	"strings"
	"time"
)

func main() {
	args := os.Args[1:]
	joined := strings.Join(args, "|")
	for _, flag := range []string{"--print", "--safe-mode", "--setting-sources||", `--settings|{"disableAllHooks":true}`, "--output-format|stream-json", "--verbose", "--include-partial-messages", "--tools||", "--disallowedTools|*", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence", "--no-chrome", "--model|sonnet"} {
		if !strings.Contains(joined, flag) {
			os.Exit(10)
		}
	}
	if os.Getenv("OPENAI_API_KEY") != "" || os.Getenv("CLAUDE_CODE_OAUTH_TOKEN") != "must-not-forward" {
		os.Exit(11)
	}
	cwd, _ := os.Getwd()
	if _, err := os.Stat(cwd + "/go.mod"); err == nil {
		os.Exit(12)
	}
	input, _ := io.ReadAll(os.Stdin)
	switch string(input) {
	case "cancel":
		time.Sleep(30 * time.Second)
	case "bad":
		fmt.Println("{bad")
	case "incomplete":
		fmt.Println(`{"type":"system","subtype":"init"}`)
	case "error":
		fmt.Println(`{"type":"result","subtype":"error_max_turns","is_error":true}`)
	case "auth":
		fmt.Println(`{"type":"assistant","error":"authentication_failed","is_api_error_message":true,"message":{"content":[{"type":"text","text":"Not logged in"}]}}`)
	case "tool":
		fmt.Println(`{"type":"assistant","message":{"content":[{"type":"tool_use"}]}}`)
	case "after":
		fmt.Println(`{"type":"result","subtype":"success","is_error":false}`)
		fmt.Println(`{"type":"system"}`)
	default:
		fmt.Println(`{"type":"system","subtype":"init"}`)
		fmt.Println(`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"secret"}}}`)
		fmt.Println(`{"type":"stream_event","parent_tool_use_id":"child","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hidden"}}}`)
		fmt.Println(`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}}`)
		fmt.Println(`{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"thinking","thinking":"secret"},{"type":"text","text":"Hello"}]}}`)
		fmt.Println(`{"type":"result","subtype":"success","is_error":false,"usage":{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":5,"cache_creation_input_tokens":7}}`)
	}
}
