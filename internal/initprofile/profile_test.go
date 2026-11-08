package initprofile

import (
	"errors"
	"testing"
	"time"

	"harness-to-api/internal/auth"
)

func TestResumeRequiresExistingMatchingKey(t *testing.T) {
	store := auth.NewStore(t.TempDir())
	token, principal, err := auth.Prepare("onboarding", []string{"codex/default"}, auth.Policy{RequestTimeout: auth.Duration(90 * time.Second), MaxRequests: 20, RequestsPerMinute: 5, MaxConcurrent: 1})
	if err != nil {
		t.Fatal(err)
	}
	expiry := principal.CreatedAt.Add(time.Hour)
	principal.Policy.ExpiresAt = &expiry
	p := Profile{Version: 1, Phase: Pending, Provider: "codex", Model: "codex/default", Port: 8787, Principal: principal}
	if _, err := Resume(store, p, token); !errors.Is(err, ErrRepair) {
		t.Fatalf("missing record: %v", err)
	}
	if err := store.InsertPrepared(token, principal); err != nil {
		t.Fatal(err)
	}
	if _, err := Resume(store, p, token); err != nil {
		t.Fatal(err)
	}
	if _, err := Resume(store, p, ""); !errors.Is(err, ErrRepair) {
		t.Fatalf("missing credential: %v", err)
	}
	altered := p
	altered.Model = "codex/other"
	if _, err := Resume(store, altered, token); !errors.Is(err, ErrRepair) {
		t.Fatalf("mismatch: %v", err)
	}
	if err := store.Revoke(principal.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := Resume(store, p, token); !errors.Is(err, ErrRepair) {
		t.Fatalf("revoked: %v", err)
	}
	if keys, err := store.List(); err != nil || len(keys) != 0 {
		t.Fatalf("recreated key: %v %v", keys, err)
	}
}
