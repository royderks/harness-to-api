package api

import (
	"context"
	"fmt"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"harness-to-api/internal/auth"
	"harness-to-api/internal/harness"
)

func policyServer(t *testing.T, policy auth.Policy, run func(context.Context, harness.Request, func(string) error) (*harness.Usage, error), max int) (*Server, string, *auth.Store, string) {
	t.Helper()
	store := auth.NewStore(t.TempDir())
	token, principal, err := store.Create("policy", []string{"codex/*"}, policy)
	if err != nil {
		t.Fatal(err)
	}
	s, err := New(Config{Keys: store, Adapters: []harness.Adapter{adapter{run}}, Timeout: time.Second, MaxConcurrent: max})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s, token, store, principal.ID
}
func TestQuotaValidationAndRestart(t *testing.T) {
	var runs atomic.Int32
	s, token, store, id := policyServer(t, auth.Policy{MaxRequests: 1}, func(ctx context.Context, r harness.Request, emit func(string) error) (*harness.Usage, error) {
		runs.Add(1)
		return normal(ctx, r, emit)
	}, 8)
	for _, body := range []string{`{}`, strings.Replace(chatBody, "codex/default", "codex/*", 1), strings.Replace(chatBody, "codex/default", "other/model", 1)} {
		if w := perform(s, token, "/v1/chat/completions", body); w.Code == 200 {
			t.Fatal(body)
		}
	}
	info, err := store.Inspect(id)
	if err != nil || info.Accepted != 0 {
		t.Fatalf("inspection: %+v %v", info, err)
	}
	var wg sync.WaitGroup
	var success, limited atomic.Int32
	for i := 0; i < 12; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			w := perform(s, token, "/v1/chat/completions", chatBody)
			if w.Code == 200 {
				success.Add(1)
			} else if w.Code == 429 && strings.Contains(w.Body.String(), "request_quota_exceeded") {
				limited.Add(1)
			} else {
				t.Errorf("%d %s", w.Code, w.Body)
			}
		}()
	}
	wg.Wait()
	if success.Load() != 1 || limited.Load() != 11 || runs.Load() != 1 {
		t.Fatalf("success=%d limited=%d runs=%d", success.Load(), limited.Load(), runs.Load())
	}
	if _, err := New(Config{Keys: store, Adapters: []harness.Adapter{adapter{normal}}}); err == nil {
		t.Fatal("second server accepted")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	s2, err := New(Config{Keys: store, Adapters: []harness.Adapter{adapter{normal}}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s2.Close() })
	if w := perform(s2, token, "/v1/chat/completions", chatBody); w.Code != 429 || !strings.Contains(w.Body.String(), "request_quota_exceeded") {
		t.Fatal(w.Code, w.Body)
	}
}
func TestRateAndConcurrencyLimits(t *testing.T) {
	for _, tc := range []struct {
		name, code string
		policy     auth.Policy
		max        int
	}{
		{"rate", "rate_limit_exceeded", auth.Policy{RequestsPerMinute: 1}, 4},
		{"key", "key_concurrency_exceeded", auth.Policy{MaxConcurrent: 1}, 4},
		{"global", "capacity_exceeded", auth.Policy{}, 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			started := make(chan struct{}, 1)
			release := make(chan struct{})
			s, token, store, id := policyServer(t, tc.policy, func(ctx context.Context, _ harness.Request, _ func(string) error) (*harness.Usage, error) {
				started <- struct{}{}
				select {
				case <-release:
				case <-ctx.Done():
				}
				return nil, ctx.Err()
			}, tc.max)
			done := make(chan struct{})
			go func() { defer close(done); perform(s, token, "/v1/chat/completions", chatBody) }()
			<-started
			w := perform(s, token, "/v1/chat/completions", chatBody)
			if w.Code != 429 || !strings.Contains(w.Body.String(), tc.code) || w.Header().Get("Retry-After") == "" {
				t.Fatal(w.Code, w.Body)
			}
			close(release)
			<-done
			info, err := store.Inspect(id)
			if err != nil || info.Accepted != 1 {
				t.Fatalf("%+v %v", info, err)
			}
		})
	}
}
func TestRevocationAndExpiryDuringRequests(t *testing.T) {
	for _, stream := range []bool{false, true} {
		for _, revoke := range []bool{false, true} {
			t.Run(fmt.Sprintf("stream=%v/revoke=%v", stream, revoke), func(t *testing.T) {
				expiry := time.Now().Add(250 * time.Millisecond)
				policy := auth.Policy{}
				if !revoke {
					policy.ExpiresAt = &expiry
				}
				started := make(chan struct{})
				s, token, store, id := policyServer(t, policy, func(ctx context.Context, _ harness.Request, _ func(string) error) (*harness.Usage, error) {
					close(started)
					<-ctx.Done()
					return nil, nil
				}, 2)
				body := chatBody
				if stream {
					body = strings.TrimSuffix(body, "}") + `,"stream":true}`
				}
				done := make(chan string, 1)
				go func() {
					w := perform(s, token, "/v1/chat/completions", body)
					done <- fmt.Sprintf("%d %s", w.Code, w.Body.String())
				}()
				<-started
				if revoke {
					if err := store.Revoke(id); err != nil {
						t.Fatal(err)
					}
				}
				select {
				case result := <-done:
					if !strings.Contains(result, "invalid_api_key") || strings.Contains(result, "[DONE]") || strings.Contains(result, `"finish_reason":"stop"`) {
						t.Fatal(result)
					}
				case <-time.After(2 * time.Second):
					t.Fatal("request did not stop")
				}
			})
		}
	}
}
func TestStrictModels(t *testing.T) {
	for _, bad := range [][]string{{"codex/*"}, {"codex/default"}, {"other/model"}} {
		if ValidateModels(bad, []string{"codex"}) == nil {
			t.Fatal(bad)
		}
	}
	if err := ValidateModels([]string{"codex/model"}, []string{"codex"}); err != nil {
		t.Fatal(err)
	}
	if err := CheckModel("codex/model", []string{"codex/*"}, []string{"codex"}, []string{"codex/model"}); err != nil {
		t.Fatal(err)
	}
	if err := CheckModel("codex/other", []string{"codex/*"}, []string{"codex"}, []string{"codex/model"}); err == nil {
		t.Fatal("accepted unlisted model")
	}
	store := auth.NewStore(t.TempDir())
	token, _, err := store.Create("strict", []string{"codex/model"}, auth.Policy{})
	if err != nil {
		t.Fatal(err)
	}
	s, err := New(Config{Keys: store, Adapters: []harness.Adapter{adapter{normal}}, AllowedModels: []string{"codex/model", "codex/other"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s.Close() })
	w := perform(s, token, "/v1/models", "")
	if w.Code != 200 || !strings.Contains(w.Body.String(), "codex/model") || strings.Contains(w.Body.String(), "codex/other") {
		t.Fatal(w.Code, w.Body)
	}
}

func TestCanceledRequestNeverSucceeds(t *testing.T) {
	for _, stream := range []bool{false, true} {
		ctx, cancel := context.WithCancel(context.Background())
		s, token, _, _ := policyServer(t, auth.Policy{}, func(_ context.Context, _ harness.Request, _ func(string) error) (*harness.Usage, error) {
			cancel()
			return nil, nil
		}, 1)
		body := chatBody
		if stream {
			body = strings.TrimSuffix(body, "}") + `,"stream":true}`
		}
		r := httptest.NewRequest("POST", "/v1/chat/completions", strings.NewReader(body)).WithContext(ctx)
		r.Header.Set("Authorization", "Bearer "+token)
		r.Header.Set("Content-Type", "application/json")
		w := httptest.NewRecorder()
		s.ServeHTTP(w, r)
		if (!stream && w.Code == 200) || strings.Contains(w.Body.String(), "[DONE]") || strings.Contains(w.Body.String(), `"finish_reason":"stop"`) {
			t.Fatalf("stream=%v: %d %s", stream, w.Code, w.Body)
		}
	}
}
func TestCorruptKeyStoreIsSanitized(t *testing.T) {
	s, token, store, id := policyServer(t, auth.Policy{}, normal, 1)
	if err := os.WriteFile(filepath.Join(store.Directory, id+".json"), []byte("secret corrupted data"), 0600); err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{"/v1/models", "/v1/chat/completions"} {
		body := ""
		if path != "/v1/models" {
			body = chatBody
		}
		w := perform(s, token, path, body)
		if w.Code != 503 || strings.Contains(w.Body.String(), "secret") {
			t.Fatalf("%s: %d %s", path, w.Code, w.Body)
		}
	}
}

func TestRateLimitPersistsAndExpiredModelsDenied(t *testing.T) {
	s, token, store, _ := policyServer(t, auth.Policy{RequestsPerMinute: 1}, normal, 2)
	if w := perform(s, token, "/v1/chat/completions", chatBody); w.Code != 200 {
		t.Fatal(w.Code, w.Body)
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	next, err := New(Config{Keys: store, Adapters: []harness.Adapter{adapter{normal}}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = next.Close() })
	if w := perform(next, token, "/v1/chat/completions", chatBody); w.Code != 429 || !strings.Contains(w.Body.String(), "rate_limit_exceeded") {
		t.Fatal(w.Code, w.Body)
	}
	expired := time.Now().Add(-time.Second)
	expiredToken, _, err := store.Create("expired", []string{"codex/*"}, auth.Policy{ExpiresAt: &expired})
	if err != nil {
		t.Fatal(err)
	}
	if w := perform(next, expiredToken, "/v1/models", ""); w.Code != 401 {
		t.Fatal(w.Code, w.Body)
	}
}
