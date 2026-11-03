package api

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"harness-to-api/internal/auth"
	"harness-to-api/internal/harness"
)

type adapter struct {
	run func(context.Context, harness.Request, func(string) error) (*harness.Usage, error)
}

func (adapter) ID() string { return "codex" }
func (a adapter) Run(ctx context.Context, r harness.Request, emit func(string) error) (*harness.Usage, error) {
	return a.run(ctx, r, emit)
}
func normal(ctx context.Context, r harness.Request, emit func(string) error) (*harness.Usage, error) {
	if err := emit("Hello "); err != nil {
		return nil, err
	}
	if err := emit("🌍"); err != nil {
		return nil, err
	}
	return &harness.Usage{PromptTokens: 5, CompletionTokens: 2, TotalTokens: 7}, nil
}
func setup(t *testing.T, run func(context.Context, harness.Request, func(string) error) (*harness.Usage, error), timeout time.Duration) (*Server, string, *auth.Store) {
	t.Helper()
	keys := auth.NewStore(t.TempDir())
	token, _, err := keys.Create("test", []string{"codex/*"}, auth.Policy{})
	if err != nil {
		t.Fatal(err)
	}
	s, err := New(Config{Keys: keys, Adapters: []harness.Adapter{adapter{run}}, Timeout: timeout, MaxConcurrent: 1})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s, token, keys
}
func perform(s *Server, token, path, body string) *httptest.ResponseRecorder {
	method := "POST"
	if body == "" {
		method = "GET"
	}
	r := httptest.NewRequest(method, path, strings.NewReader(body))
	r.Header.Set("Content-Type", "application/json")
	if token != "" {
		r.Header.Set("Authorization", "Bearer "+token)
	}
	w := httptest.NewRecorder()
	s.ServeHTTP(w, r)
	return w
}

const chatBody = `{"model":"codex/default","messages":[{"role":"user","content":"hi"}]}`

func TestAuthenticationAndRevocation(t *testing.T) {
	s, key, keys := setup(t, normal, time.Second)
	if w := perform(s, "", "/v1/models", ""); w.Code != 401 {
		t.Fatal(w.Code)
	}
	if w := perform(s, key, "/v1/models", ""); w.Code != 200 || !strings.Contains(w.Body.String(), "codex/default") {
		t.Fatal(w.Body.String())
	}
	list, _ := keys.List()
	if err := keys.Revoke(list[0].ID); err != nil {
		t.Fatal(err)
	}
	if w := perform(s, key, "/v1/chat/completions", chatBody); w.Code != 401 {
		t.Fatal(w.Code)
	}
	if w := perform(s, "", "/healthz", ""); w.Code != 200 {
		t.Fatal(w.Code)
	}
}
func TestCompletions(t *testing.T) {
	s, key, _ := setup(t, normal, time.Second)
	for _, tc := range []struct{ path, body, object string }{
		{"/v1/chat/completions", chatBody, "chat.completion"},
		{"/v1/completions", `{"model":"codex/default","prompt":"hi"}`, "text_completion"},
	} {
		t.Run(tc.object, func(t *testing.T) {
			w := perform(s, key, tc.path, tc.body)
			if w.Code != 200 {
				t.Fatalf("%d %s", w.Code, w.Body)
			}
			var body map[string]any
			if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
				t.Fatal(err)
			}
			if body["object"] != tc.object || !strings.Contains(w.Body.String(), "Hello 🌍") || !strings.Contains(w.Body.String(), `"total_tokens":7`) {
				t.Fatal(w.Body.String())
			}
		})
	}
}
func TestUnsupportedInputs(t *testing.T) {
	s, key, _ := setup(t, normal, time.Second)
	for _, body := range []string{
		`null`, `{`, `{} `, chatBody + ` {}`, `{"model":"codex/default","messages":[]}`,
		`{"model":"codex/default","messages":[{"role":"tool","content":"hi"}]}`,
		`{"model":"codex/default","messages":[{"role":"user","content":null}]}`,
		`{"model":"codex/default","messages":[{"role":"user","content":[{"type":"image_url","image_url":{}}]}]}`,
		strings.TrimSuffix(chatBody, "}") + `,"temperature":0.2}`, strings.TrimSuffix(chatBody, "}") + `,"tools":[]}`,
		strings.TrimSuffix(chatBody, "}") + `,"stream_options":{"include_usage":true}}`,
	} {
		w := perform(s, key, "/v1/chat/completions", body)
		if w.Code != 400 {
			t.Fatalf("accepted %s: %d %s", body, w.Code, w.Body)
		}
	}
	if w := perform(s, key, "/v1/chat/completions", strings.Replace(chatBody, "codex/default", "antigravity/default", 1)); w.Code != 403 {
		t.Fatal(w.Code)
	}
	if w := perform(s, key, "/v1/chat/completions", strings.Repeat("x", 1024*1024+1)); w.Code != 413 {
		t.Fatal(w.Code)
	}
}
func TestStreaming(t *testing.T) {
	s, key, _ := setup(t, normal, time.Second)
	body := strings.TrimSuffix(chatBody, "}") + `,"stream":true,"stream_options":{"include_usage":true}}`
	w := perform(s, key, "/v1/chat/completions", body)
	if w.Code != 200 || w.Header().Get("Content-Type") != "text/event-stream" {
		t.Fatal(w.Code, w.Header())
	}
	text := w.Body.String()
	if !strings.Contains(text, `"role":"assistant"`) || !strings.Contains(text, `"content":"🌍"`) || !strings.Contains(text, `"choices":[],`) || !strings.HasSuffix(text, "data: [DONE]\n\n") {
		t.Fatal(text)
	}
	if strings.Count(text, `"total_tokens":7`) != 1 {
		t.Fatal("usage duplicated")
	}
}
func TestFailureAndTimeout(t *testing.T) {
	for _, stream := range []bool{false, true} {
		for _, timeout := range []bool{false, true} {
			limit := time.Second
			if timeout {
				limit = 10 * time.Millisecond
			}
			s, key, _ := setup(t, func(ctx context.Context, _ harness.Request, _ func(string) error) (*harness.Usage, error) {
				if timeout {
					<-ctx.Done()
					return nil, ctx.Err()
				}
				return nil, errors.New("secret provider credential")
			}, limit)
			body := chatBody
			if stream {
				body = strings.TrimSuffix(body, "}") + `,"stream":true}`
			}
			w := perform(s, key, "/v1/chat/completions", body)
			want := 502
			if timeout {
				want = 504
			}
			if stream {
				want = 200
			}
			if w.Code != want || strings.Contains(w.Body.String(), "secret") || strings.Contains(w.Body.String(), "[DONE]") {
				t.Fatalf("%d %s", w.Code, w.Body)
			}
			code := "harness_failed"
			if timeout {
				code = "harness_timeout"
			}
			if !strings.Contains(w.Body.String(), code) {
				t.Fatal(w.Body.String())
			}
		}
	}
}
func TestBusyAndCancellation(t *testing.T) {
	started := make(chan struct{}, 1)
	stopped := make(chan struct{}, 1)
	s, key, _ := setup(t, func(ctx context.Context, _ harness.Request, _ func(string) error) (*harness.Usage, error) {
		started <- struct{}{}
		<-ctx.Done()
		stopped <- struct{}{}
		return nil, ctx.Err()
	}, time.Second)
	server := httptest.NewServer(s)
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	body := strings.TrimSuffix(chatBody, "}") + `,"stream":true}`
	req, _ := http.NewRequestWithContext(ctx, "POST", server.URL+"/v1/chat/completions", strings.NewReader(body))
	req.Header.Set("Authorization", "Bearer "+key)
	req.Header.Set("Content-Type", "application/json")
	response, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer response.Body.Close()
	<-started
	w := perform(s, key, "/v1/chat/completions", chatBody)
	if w.Code != 429 {
		t.Fatal(w.Code)
	}
	cancel()
	response.Body.Close()
	select {
	case <-stopped:
	case <-time.After(time.Second):
		t.Fatal("disconnect did not cancel harness")
	}
}
func TestRolesAndTextBlocks(t *testing.T) {
	s, key, _ := setup(t, func(_ context.Context, r harness.Request, emit func(string) error) (*harness.Usage, error) {
		if !strings.Contains(r.Prompt, `"role":"system"`) || !strings.Contains(r.Prompt, `"content":"ab"`) {
			t.Error(r.Prompt)
		}
		return nil, emit("ok")
	}, time.Second)
	w := perform(s, key, "/v1/chat/completions", `{"model":"codex/default","messages":[{"role":"system","content":"Be brief"},{"role":"user","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}]}`)
	if w.Code != 200 {
		t.Fatal(w.Body.String())
	}
}
func TestHTTPStreamingFlush(t *testing.T) {
	release := make(chan struct{})
	s, key, _ := setup(t, func(ctx context.Context, _ harness.Request, emit func(string) error) (*harness.Usage, error) {
		if err := emit("first"); err != nil {
			return nil, err
		}
		select {
		case <-release:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
		return nil, emit("second")
	}, time.Second)
	server := httptest.NewServer(s)
	defer server.Close()
	req, _ := http.NewRequest("POST", server.URL+"/v1/completions", strings.NewReader(`{"model":"codex/default","prompt":"hi","stream":true}`))
	req.Header.Set("Authorization", "Bearer "+key)
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	buffer := make([]byte, 4096)
	n, err := res.Body.Read(buffer)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(buffer[:n]), "first") {
		t.Fatal(string(buffer[:n]))
	}
	close(release)
	rest, _ := io.ReadAll(res.Body)
	if !strings.Contains(string(rest), "second") || !strings.Contains(string(rest), "[DONE]") {
		t.Fatal(string(rest))
	}
}
