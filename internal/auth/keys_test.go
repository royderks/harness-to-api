package auth

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func TestKeyLifecycle(t *testing.T) {
	s := NewStore(t.TempDir())
	token, p, err := s.Create("sandbox", []string{"codex/*"})
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(filepath.Join(s.Directory, p.ID+".json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), token) {
		t.Fatal("plaintext token stored")
	}
	got, err := s.Authenticate(token)
	if err != nil || got.ID != p.ID {
		t.Fatalf("authenticate: %v", err)
	}
	if !got.Allows("codex/default") || got.Allows("antigravity/default") || got.Allows("codexevil/default") {
		t.Fatal("scope boundary failed")
	}
	for _, bad := range []string{"", token + "x", "h2a_../../etc/passwd_x", token[:len(token)-2] + "xx"} {
		if _, err = s.Authenticate(bad); err == nil {
			t.Fatal("accepted bad key")
		}
	}
	list, err := s.List()
	if err != nil || len(list) != 1 {
		t.Fatalf("list: %v", err)
	}
	if err = s.Revoke(p.ID); err != nil {
		t.Fatal(err)
	}
	if _, err = s.Authenticate(token); err == nil {
		t.Fatal("revoked key accepted")
	}
}
func TestConcurrentKeys(t *testing.T) {
	s := NewStore(t.TempDir())
	var wg sync.WaitGroup
	for range 16 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, _, err := s.Create("parallel", []string{"*"}); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	keys, err := s.List()
	if err != nil || len(keys) != 16 {
		t.Fatalf("lost keys: %d, %v", len(keys), err)
	}
}

func TestIBMBobModelScopes(t *testing.T) {
	model := "ibm-bob/openai/gpt-oss-20b"
	if !ModelPattern.MatchString(model) {
		t.Fatal("slash model rejected")
	}
	for _, tc := range []struct {
		scope   string
		allowed bool
	}{{"ibm-bob/*", true}, {model, true}, {"ibm-bob/premium", false}, {"codex/*", false}} {
		if !scopePattern.MatchString(tc.scope) {
			t.Fatalf("invalid scope %q", tc.scope)
		}
		if got := (Principal{Models: []string{tc.scope}}).Allows(model); got != tc.allowed {
			t.Fatalf("scope %q: %v", tc.scope, got)
		}
	}
}
