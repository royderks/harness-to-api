package main

import (
	"fmt"
	"io"
	"os"
	"strings"
	"time"
)

func main() {
	input, _ := io.ReadAll(os.Stdin)
	args := strings.Join(os.Args[1:], "|")
	if !strings.HasPrefix(args, "run|--standalone|--format|json") || os.Getenv("OPENAI_API_KEY") != "" || os.Getenv("OPENCODE_DISABLE_PROJECT_CONFIG") != "1" || !strings.Contains(os.Getenv("OPENCODE_CONFIG_CONTENT"), `"tool.use"`) || os.Getenv("XDG_CONFIG_HOME") == "" {
		os.Exit(3)
	}
	switch string(input) {
	case "good":
		if !strings.Contains(args, "--model|anthropic/claude/example") {
			os.Exit(4)
		}
		fmt.Println(`{"type":"step_start","part":{"type":"step-start"}}`)
		fmt.Println(`{"type":"reasoning","part":{"type":"reasoning","text":"private"}}`)
		fmt.Println(`{"type":"text","part":{"type":"text","text":"hello"}}`)
		fmt.Println(`{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":10,"output":3,"reasoning":2,"cache":{"read":4,"write":1}}}}`)
	case "error":
		fmt.Println(`{"type":"error","error":{"message":"secret"}}`)
	case "incomplete":
		fmt.Println(`{"type":"text","part":{"type":"text","text":"partial"}}`)
	case "malformed":
		fmt.Println(`{`)
	case "tool":
		fmt.Println(`{"type":"tool_use","part":{"type":"tool"}}`)
	case "badfinish":
		fmt.Println(`{"type":"step_finish","part":{"type":"step-finish","reason":"tool-calls"}}`)
	case "cancel":
		time.Sleep(30 * time.Second)
	case "emit":
		fmt.Println(`{"type":"text","part":{"type":"text","text":"hi"}}`)
		time.Sleep(30 * time.Second)
	default:
		os.Exit(5)
	}
}
