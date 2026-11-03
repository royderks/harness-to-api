package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
)

func TestPolicyCLI(t *testing.T) {
	state := t.TempDir()
	for _, flags := range [][]string{{"--ttl", "0s"}, {"--ttl", "-1s"}, {"--ttl", "1h", "--expires-at", "2030-01-01T00:00:00Z"}, {"--request-timeout", "0s"}, {"--request-timeout", "2h"}, {"--max-requests", "-1"}, {"--max-requests", "18446744073709551616"}, {"--rate-limit", "0/min"}, {"--rate-limit", "100001/min"}, {"--max-concurrent", "0"}, {"--max-concurrent", "129"}, {"--validate", "--models", "codex/*", "--allowed-models", "codex/foo"}, {"--validate", "--models", "codex/default", "--allowed-models", "codex/default"}, {"--validate", "--models", "codex/foo"}, {"--validate", "--models", "codex/foo", "--allowed-models", "gemini/foo"}} {
		args := append([]string{"keys", "create", "--state-dir", state}, flags...)
		out, _, err := cli(t, args...)
		if err == nil || out != "" {
			t.Errorf("accepted %v: %q %v", flags, out, err)
		}
	}
	out, stderr, err := cli(t, "keys", "create", "--state-dir", state, "--models", "codex/foo", "--validate", "--allowed-models", "codex/foo", "--ttl", "1h", "--request-timeout", "5s", "--max-requests", "3", "--rate-limit", "2/min", "--max-concurrent", "1")
	if err != nil {
		t.Fatalf("create: %s %v", stderr, err)
	}
	token := strings.TrimSpace(out)
	if !strings.HasPrefix(token, "h2a_") || strings.Count(out, "\n") != 1 {
		t.Fatalf("secret output %q", out)
	}
	list, _, err := cli(t, "keys", "list", "--state-dir", state)
	if err != nil {
		t.Fatal(err)
	}
	var keys []struct {
		ID string `json:"id"`
	}
	if json.Unmarshal([]byte(list), &keys) != nil || len(keys) != 1 {
		t.Fatalf("invalid keys %s", list)
	}
	before, stderr, err := cli(t, "keys", "inspect", "--state-dir", state, keys[0].ID)
	if err != nil {
		t.Fatalf("inspect: %s", stderr)
	}
	explain, stderr, err := cli(t, "keys", "explain", "--state-dir", state, "--model", "codex/foo", "--allowed-models", "codex/foo", keys[0].ID)
	if err != nil {
		t.Fatalf("explain: %s", stderr)
	}
	after, _, err := cli(t, "keys", "inspect", "--state-dir", state, keys[0].ID)
	if err != nil || before != after {
		t.Fatalf("explain mutated usage: %s %s", before, after)
	}
	if strings.Contains(before+explain, token) || !strings.Contains(explain, `"account_access":"unknown"`) || !strings.Contains(explain, `"model_allowed":true`) {
		t.Fatalf("bad snapshot: %s %s", before, explain)
	}
}

func TestProbeCLI(t *testing.T) {
	const secret = "h2a_secret_probe"
	t.Setenv("OPENAI_API_KEY", secret)
	var redirected atomic.Bool
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { redirected.Store(true) }))
	defer target.Close()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" || r.Header.Get("Authorization") != "Bearer "+secret {
			t.Errorf("unexpected probe request")
		}
		if r.URL.Query().Get("redirect") == "yes" {
			http.Redirect(w, r, target.URL, http.StatusTemporaryRedirect)
			return
		}
		w.WriteHeader(http.StatusBadRequest)
		_, _ = w.Write([]byte(secret + " malicious provider output"))
	}))
	defer server.Close()
	out, stderr, err := cli(t, "probe", "--base-url", server.URL+"/v1", "--model", "codex/foo")
	if err == nil || strings.Contains(out+stderr, secret) || strings.Contains(out+stderr, "malicious") || !strings.Contains(out, `"available":false`) {
		t.Fatalf("probe: %q %q %v", out, stderr, err)
	}
	success := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var request map[string]any
		if json.NewDecoder(r.Body).Decode(&request) != nil || request["max_tokens"] != nil || request["model"] != "codex/foo" {
			t.Error("probe sent an unsupported request")
		}
		_, _ = w.Write([]byte(`{"object":"chat.completion","model":"codex/foo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"` + secret + ` untrusted response"}}]}`))
	}))
	defer success.Close()
	out, stderr, err = cli(t, "probe", "--base-url", success.URL+"/v1", "--model", "codex/foo")
	if err != nil || !strings.Contains(out, `"available":true`) || strings.Contains(out+stderr, secret) || strings.Contains(out+stderr, "untrusted") {
		t.Fatalf("successful probe: %q %q %v", out, stderr, err)
	}
	redirect := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, target.URL, http.StatusTemporaryRedirect)
	}))
	defer redirect.Close()
	out, stderr, err = cli(t, "probe", "--base-url", redirect.URL+"/v1", "--model", "codex/foo")
	if err == nil || redirected.Load() || strings.Contains(out+stderr, secret) {
		t.Fatalf("redirect: %q %q %v", out, stderr, err)
	}
	out, stderr, err = cli(t, "probe", "--base-url", "http://example.com/v1", "--model", "codex/foo")
	if err == nil || out != "" || strings.Contains(stderr, secret) {
		t.Fatalf("remote HTTP: %q %q %v", out, stderr, err)
	}
	for _, response := range []string{"OK", `{}`, `{"error":{"message":"secret"}}`, `{"object":"chat.completion","model":"codex/wrong","choices":[]}`} {
		invalid := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(response)) }))
		out, _, err = cli(t, "probe", "--base-url", invalid.URL+"/v1", "--model", "codex/foo")
		invalid.Close()
		if err == nil || !strings.Contains(out, `"available":false`) {
			t.Fatalf("invalid success accepted: %s", out)
		}
	}
}
