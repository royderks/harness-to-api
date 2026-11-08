package initprofile

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"harness-to-api/internal/auth"
)

func testProfile(t *testing.T) (Profile, string) {
	t.Helper()
	token, p, err := auth.Prepare("onboarding", []string{"codex/default"}, auth.Policy{RequestTimeout: auth.Duration(90 * time.Second), MaxRequests: 20, RequestsPerMinute: 5, MaxConcurrent: 1})
	if err != nil {
		t.Fatal(err)
	}
	expiry := p.CreatedAt.Add(time.Hour)
	p.Policy.ExpiresAt = &expiry
	return Profile{Version: 1, Phase: Pending, Provider: "codex", Model: "codex/default", Port: 8787, Principal: p}, token
}
func newTestStore(t *testing.T) (*Store, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "state")
	s, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := s.Close(); err != nil {
			t.Error(err)
		}
	})
	return s, path
}
func TestStorageLifecycle(t *testing.T) {
	s, _ := newTestStore(t)
	p, token := testProfile(t)
	got, secret, err := s.Load()
	if err != nil || got != nil || secret != "" {
		t.Fatalf("empty: profile=%v credential_present=%t error=%v", got, secret != "", err)
	}
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	got, secret, err = s.Load()
	if err != nil || got == nil || got.Phase != Pending || secret != "" {
		t.Fatalf("pending: profile=%#v credential_present=%t error=%v", got, secret != "", err)
	}
	if err := s.SaveCredential(token); err != nil {
		t.Fatal(err)
	}
	if err := s.SaveCredential(token); err == nil {
		t.Fatal("overwrote credential")
	}
	got, secret, err = s.Load()
	if err != nil || got == nil || secret != token {
		t.Fatalf("load: %#v %v", got, err)
	}
	p.Phase = Ready
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	got, secret, err = s.Load()
	if err != nil || got == nil || got.Phase != Ready || secret != token {
		t.Fatalf("ready: %#v %v", got, err)
	}
	if err := s.RemoveCredential(); err != nil {
		t.Fatal(err)
	}
	if err := s.RemoveCredential(); err != nil {
		t.Fatal(err)
	}
	if err := s.RemoveProfile(); err != nil {
		t.Fatal(err)
	}
	if err := s.RemoveProfile(); err != nil {
		t.Fatal(err)
	}
	got, secret, err = s.Load()
	if err != nil || got != nil || secret != "" {
		t.Fatalf("removed: %#v %v", got, err)
	}
}
func TestStorageRejectsOrphanAndMalformedData(t *testing.T) {
	s, _ := newTestStore(t)
	p, token := testProfile(t)
	if err := s.SaveCredential(token); err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.Load(); !errors.Is(err, ErrRepair) {
		t.Fatalf("orphan: %v", err)
	}
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	// Rewrite only through the protected creation path to preserve native security.
	if err := s.RemoveProfile(); err != nil {
		t.Fatal(err)
	}
	for _, b := range []string{`{"version":1,"unknown":true}`, strings.Repeat("a", maxProfileBytes+1), `{} {}`} {
		if err := s.publish("profile.json", []byte(b), false); err != nil {
			t.Fatal(err)
		}
		if _, _, err := s.Load(); !errors.Is(err, ErrRepair) {
			t.Fatalf("bad profile: %v", err)
		}
	}
}
func TestStorageRetainsDirectoryIdentity(t *testing.T) {
	s, state := newTestStore(t)
	p, token := testProfile(t)
	// On systems where an open directory can be renamed, writes must stay in the
	// opened directory rather than a replacement at its original path.
	old := filepath.Join(state, "old-init")
	if err := os.Rename(filepath.Join(state, "init"), old); err != nil {
		t.Skipf("directory rename blocked: %v", err)
	}
	if err := os.Mkdir(filepath.Join(state, "init"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	if err := s.SaveCredential(token); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(state, "init", "client.key")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("wrote replacement: %v", err)
	}
	if _, err := os.Stat(filepath.Join(old, "client.key")); err != nil {
		t.Fatal(err)
	}
}
