package auth

import (
	"testing"
	"time"
)

func TestPreparedKeyInsertIsNotAnUpsert(t *testing.T) {
	s := NewStore(t.TempDir())
	expiry := time.Now().Add(time.Hour).UTC()
	token, p, err := Prepare("onboarding", []string{"codex/default"}, Policy{ExpiresAt: &expiry, MaxRequests: 20})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := s.Authenticate(token); err == nil {
		t.Fatal("prepared key was persisted")
	}
	if err := s.InsertPrepared(token, p); err != nil {
		t.Fatal(err)
	}
	if err := s.InsertPrepared(token, p); err == nil {
		t.Fatal("duplicate insert succeeded")
	}
	got, err := s.Authenticate(token)
	if err != nil || got.ID != p.ID {
		t.Fatalf("authenticate: %v", err)
	}
	other, _, err := Prepare("other", []string{"codex/default"}, Policy{})
	if err != nil {
		t.Fatal(err)
	}
	if err := s.InsertPrepared(other, p); err == nil {
		t.Fatal("mismatched token accepted")
	}
}
