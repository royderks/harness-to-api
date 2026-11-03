package harness

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
)

// Exercises the installed Pi and pi-bob packages against a local fake IBM endpoint.
func TestPiBobRuntime(t *testing.T) {
	bin, extension := os.Getenv("H2A_PI_BIN"), os.Getenv("H2A_PI_BOB_EXTENSION")
	if bin == "" || extension == "" {
		t.Skip("set H2A_PI_BIN and H2A_PI_BOB_EXTENSION for a real Pi runtime, local endpoint check")
	}
	endpoint := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if r.URL.Path != "/chat/completions" || r.Method != http.MethodPost || json.NewDecoder(r.Body).Decode(&body) != nil {
			t.Error("unexpected Pi request", r.Method, r.URL.Path)
			http.Error(w, "unexpected request", 400)
			return
		}
		if (body["model"] != "premium" && body["model"] != "openai/gpt-oss-20b") || len(body["messages"].([]any)) == 0 || r.Header.Get("Authorization") != "Apikey local-test-key" {
			t.Error("incorrect model, prompt or provider credential")
		}
		if list, ok := body["tools"].([]any); ok && len(list) > 0 {
			t.Error("tools were enabled")
		}
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprint(w, "data: {\"id\":\"fixture\",\"object\":\"chat.completion.chunk\",\"model\":\"premium\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Pi fixture OK\"},\"finish_reason\":null}]}\n\n")
		fmt.Fprint(w, "data: {\"id\":\"fixture\",\"object\":\"chat.completion.chunk\",\"model\":\"premium\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3,\"total_tokens\":8}}\n\ndata: [DONE]\n\n")
	}))
	defer endpoint.Close()
	for name, value := range map[string]string{"HOME": t.TempDir(), "PI_CODING_AGENT_DIR": t.TempDir(), "PI_OFFLINE": "1", "IBM_BOB_READ_BOBSHELL_SETTINGS": "false", "IBM_BOB_API_KEY": "local-test-key", "IBM_BOB_BASE_URL": endpoint.URL, "IBM_BOB_DISCOVER_MODELS": "false", "IBM_BOB_MODELS": "premium,openai/gpt-oss-20b", "IBM_BOB_INSTANCE_ID": "local-test", "IBM_BOB_TEAM_ID": "local-test"} {
		t.Setenv(name, value)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	for _, model := range []string{"premium", "openai/gpt-oss-20b"} {
		var output strings.Builder
		usage, err := (IBMBob{Binary: bin, Extension: extension}).Run(ctx, Request{Model: model, Prompt: "Say hello"}, func(s string) error { output.WriteString(s); return nil })
		if err != nil || output.String() != "Pi fixture OK" || usage == nil || usage.TotalTokens != 8 {
			t.Fatalf("output=%q usage=%+v err=%v", output.String(), usage, err)
		}
	}
}
