package initprofile

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
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

func TestResumeReportsReasons(t *testing.T) {
	t.Run("missing and mismatch", func(t *testing.T) {
		s := auth.NewStore(t.TempDir())
		p, token := testProfile(t)
		if _, err := Resume(s, p, token); !errors.Is(err, ErrMissing) {
			t.Fatalf("missing: %v", err)
		}
		if err := s.InsertPrepared(token, p.Principal); err != nil {
			t.Fatal(err)
		}
		other, _, err := auth.Prepare("onboarding", []string{"codex/default"}, p.Principal.Policy)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := Resume(s, p, other); !errors.Is(err, ErrMismatch) {
			t.Fatalf("mismatch: %v", err)
		}
		if _, err := Resume(s, p, ""); !errors.Is(err, ErrMissing) {
			t.Fatalf("credential: %v", err)
		}
	})
	t.Run("expired", func(t *testing.T) {
		s := auth.NewStore(t.TempDir())
		p, token := testProfile(t)
		p.Principal.CreatedAt = time.Now().UTC().Add(-2 * time.Hour)
		expiry := p.Principal.CreatedAt.Add(time.Hour)
		p.Principal.Policy.ExpiresAt = &expiry
		if err := s.InsertPrepared(token, p.Principal); err != nil {
			t.Fatal(err)
		}
		if _, err := Resume(s, p, token); !errors.Is(err, ErrExpired) {
			t.Fatalf("expired: %v", err)
		}
	})
	t.Run("exhausted", func(t *testing.T) {
		s := auth.NewStore(t.TempDir())
		p, token := testProfile(t)
		if err := s.InsertPrepared(token, p.Principal); err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(s.Directory, p.Principal.ID+".json")
		b, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		var record map[string]json.RawMessage
		if err = json.Unmarshal(b, &record); err != nil {
			t.Fatal(err)
		}
		record["usage"], err = json.Marshal(map[string]any{"accepted": 20, "last_used_at": time.Now().UTC()})
		if err != nil {
			t.Fatal(err)
		}
		b, err = json.Marshal(record)
		if err != nil {
			t.Fatal(err)
		}
		if err = os.WriteFile(path, b, 0600); err != nil {
			t.Fatal(err)
		}
		if _, err = Resume(s, p, token); !errors.Is(err, ErrExhausted) {
			t.Fatalf("exhausted: %v", err)
		}
	})
}

func TestVerifyRepairIdentity(t *testing.T) {
	for _, scenario := range []string{"matching", "missing credential", "revoked", "expired", "discarding", "other id", "wrong secret", "manual record"} {
		t.Run(scenario, func(t *testing.T) {
			s := auth.NewStore(t.TempDir())
			p, token := testProfile(t)
			if scenario == "expired" {
				p.Principal.CreatedAt = time.Now().UTC().Add(-2 * time.Hour)
				expiry := p.Principal.CreatedAt.Add(time.Hour)
				p.Principal.Policy.ExpiresAt = &expiry
			}
			if err := s.InsertPrepared(token, p.Principal); err != nil {
				t.Fatal(err)
			}
			wantMismatch := false
			switch scenario {
			case "missing credential":
				token = ""
			case "revoked":
				if err := s.Revoke(p.Principal.ID); err != nil {
					t.Fatal(err)
				}
			case "discarding":
				p.Phase = Discarding
			case "other id":
				_, token = testProfile(t)
				wantMismatch = true
			case "wrong secret":
				_, other := testProfile(t)
				token = token[:37] + other[37:]
				wantMismatch = true
			case "manual record":
				_, manual, err := s.Create("manual", []string{"codex/*"}, auth.Policy{})
				if err != nil {
					t.Fatal(err)
				}
				p.Principal.ID = manual.ID
				token = ""
				wantMismatch = true
			}
			before, err := s.List()
			if err != nil {
				t.Fatal(err)
			}
			err = VerifyRepair(s, p, token)
			if wantMismatch && !errors.Is(err, ErrMismatch) || !wantMismatch && err != nil {
				t.Fatalf("unexpected repair validation: %v", err)
			}
			after, err := s.List()
			if err != nil || !reflect.DeepEqual(before, after) {
				t.Fatal("validation changed keys")
			}
		})
	}
}

func TestLoadRejectsConflictingCredentialIdentity(t *testing.T) {
	s, _ := newTestStore(t)
	p, _ := testProfile(t)
	_, other := testProfile(t)
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	if err := s.SaveCredential(other); err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.Load(); !errors.Is(err, ErrRepair) {
		t.Fatalf("conflicting identity: %v", err)
	}
}
