//go:build linux || darwin || windows

package auth

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func TestAdmissionDurabilityAndLimits(t *testing.T) {
	s := NewStore(t.TempDir())
	token, p, e := s.Create("test", []string{"*"}, Policy{MaxRequests: 2, RequestsPerMinute: 2, MaxConcurrent: 1})
	if e != nil {
		t.Fatal(e)
	}
	a, e := s.OpenAdmission(time.Second, 1)
	if e != nil {
		t.Fatal(e)
	}
	l, e := a.Admit(context.Background(), token, "codex/a")
	if e != nil {
		t.Fatal(e)
	}
	if _, e = a.Admit(context.Background(), token, "codex/a"); e == nil {
		t.Fatal("capacity accepted")
	}
	l.Close()
	l, e = a.Admit(context.Background(), token, "codex/a")
	if e != nil {
		t.Fatal(e)
	}
	l.Close()
	if e = a.Close(); e != nil {
		t.Fatal(e)
	}
	a, e = s.OpenAdmission(0, 0)
	if e != nil {
		t.Fatal(e)
	}
	defer a.Close()
	if _, e = a.Admit(context.Background(), token, "codex/a"); e == nil {
		t.Fatal("quota reset")
	}
	i, e := s.Inspect(p.ID)
	if e != nil || i.Accepted != 2 || i.RecentRequests != 2 || *i.RemainingRequests != 0 {
		t.Fatalf("inspection %+v %v", i, e)
	}
}
func TestQuotaRaceAndRate(t *testing.T) {
	s := NewStore(t.TempDir())
	token, _, e := s.Create("race", []string{"*"}, Policy{MaxRequests: 1})
	if e != nil {
		t.Fatal(e)
	}
	a, e := s.OpenAdmission(0, 0)
	if e != nil {
		t.Fatal(e)
	}
	defer a.Close()
	var wg sync.WaitGroup
	var mu sync.Mutex
	n := 0
	for range 20 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			l, e := a.Admit(context.Background(), token, "codex/a")
			if e == nil {
				mu.Lock()
				n++
				mu.Unlock()
				l.Close()
			}
		}()
	}
	wg.Wait()
	if n != 1 {
		t.Fatalf("accepted %d", n)
	}
	token, _, e = s.Create("rate", []string{"*"}, Policy{RequestsPerMinute: 1})
	if e != nil {
		t.Fatal(e)
	}
	l, e := a.Admit(context.Background(), token, "codex/a")
	if e != nil {
		t.Fatal(e)
	}
	l.Close()
	_, e = a.Admit(context.Background(), token, "codex/a")
	var limit *LimitError
	if !errors.As(e, &limit) || limit.Code != "rate_limit_exceeded" || limit.RetryAfter <= 0 {
		t.Fatalf("rate: %v", e)
	}
}
func TestExpiredCorruptAndClose(t *testing.T) {
	s := NewStore(t.TempDir())
	past := time.Now().Add(-time.Second)
	token, _, e := s.Create("expired", []string{"*"}, Policy{ExpiresAt: &past})
	if e != nil {
		t.Fatal(e)
	}
	if _, e = s.Authenticate(token); !errors.Is(e, ErrExpiredKey) {
		t.Fatal(e)
	}
	token, p, e := s.Create("live", []string{"*"}, Policy{})
	if e != nil {
		t.Fatal(e)
	}
	a, e := s.OpenAdmission(0, 0)
	if e != nil {
		t.Fatal(e)
	}
	l, e := a.Admit(context.Background(), token, "codex/a")
	if e != nil {
		t.Fatal(e)
	}
	done := make(chan struct{})
	go func() { a.Close(); close(done) }()
	select {
	case <-l.Context().Done():
	case <-time.After(time.Second):
		t.Fatal("close did not cancel")
	}
	select {
	case <-done:
		t.Fatal("close released before lease")
	default:
	}
	l.Close()
	<-done
	if e = os.WriteFile(filepath.Join(s.Directory, p.ID+".json"), []byte(`{"bad":true}`), 0600); e != nil {
		t.Fatal(e)
	}
	if _, e = s.Inspect(p.ID); e == nil {
		t.Fatal("corrupt record accepted")
	}
}
func TestProcessLockAndRevoke(t *testing.T) {
	if dir := os.Getenv("AUTH_HELPER_DIR"); dir != "" {
		s := NewStore(dir)
		switch os.Getenv("AUTH_HELPER_ACTION") {
		case "lock":
			if a, e := s.OpenAdmission(0, 0); e == nil {
				a.Close()
				os.Exit(3)
			}
		case "revoke":
			if e := s.Revoke(os.Getenv("AUTH_HELPER_ID")); e != nil {
				os.Exit(4)
			}
		}
		return
	}
	dir := t.TempDir()
	s := NewStore(dir)
	token, p, e := s.Create("process", []string{"*"}, Policy{})
	if e != nil {
		t.Fatal(e)
	}
	a, e := s.OpenAdmission(0, 0)
	if e != nil {
		t.Fatal(e)
	}
	defer a.Close()
	l, e := a.Admit(context.Background(), token, "codex/a")
	if e != nil {
		t.Fatal(e)
	}
	defer l.Close()
	for _, action := range []string{"lock", "revoke"} {
		cmd := exec.Command(os.Args[0], "-test.run=^TestProcessLockAndRevoke$")
		cmd.Env = append(os.Environ(), "AUTH_HELPER_DIR="+dir, "AUTH_HELPER_ACTION="+action, "AUTH_HELPER_ID="+p.ID)
		if out, e := cmd.CombinedOutput(); e != nil {
			t.Fatalf("%s: %v %s", action, e, out)
		}
	}
	select {
	case <-l.Context().Done():
		if !errors.Is(context.Cause(l.Context()), ErrRevokedKey) {
			t.Fatal(context.Cause(l.Context()))
		}
	case <-time.After(3 * time.Second):
		t.Fatal("revoke did not cancel")
	}
}

func TestKeyConcurrencyAndExpiry(t *testing.T) {
	s := NewStore(t.TempDir())
	end := time.Now().Add(250 * time.Millisecond)
	token, _, err := s.Create("short", []string{"*"}, Policy{ExpiresAt: &end, MaxConcurrent: 1})
	if err != nil {
		t.Fatal(err)
	}
	a, err := s.OpenAdmission(0, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	l, err := a.Admit(context.Background(), token, "codex/a")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	_, err = a.Admit(context.Background(), token, "codex/a")
	var limit *LimitError
	if !errors.As(err, &limit) || limit.Code != "key_concurrency_exceeded" {
		t.Fatalf("concurrency: %v", err)
	}
	select {
	case <-l.Context().Done():
		if !errors.Is(context.Cause(l.Context()), ErrExpiredKey) {
			t.Fatalf("cause: %v", context.Cause(l.Context()))
		}
	case <-time.After(2 * time.Second):
		t.Fatal("expiry did not cancel")
	}
}

func TestRollingWindowAndClockRollback(t *testing.T) {
	s := NewStore(t.TempDir())
	token, p, err := s.Create("window", []string{"codex/*"}, Policy{MaxRequests: 3, RequestsPerMinute: 1})
	if err != nil {
		t.Fatal(err)
	}
	r, err := s.read(p.ID)
	if err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-2 * time.Minute)
	r.Usage = usage{Accepted: 2, LastUsedAt: &old, Recent: []time.Time{old}}
	if err := s.write(r); err != nil {
		t.Fatal(err)
	}
	a, err := s.OpenAdmission(time.Second, 2)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	lease, err := a.Admit(context.Background(), token, "codex/model")
	if err != nil {
		t.Fatal(err)
	}
	lease.Close()
	info, err := s.Inspect(p.ID)
	if err != nil || info.Accepted != 3 || info.RecentRequests != 1 {
		t.Fatalf("window reset lifetime quota: %+v %v", info, err)
	}
	token, p, err = s.Create("rollback", []string{"codex/*"}, Policy{RequestsPerMinute: 2})
	if err != nil {
		t.Fatal(err)
	}
	r, err = s.read(p.ID)
	if err != nil {
		t.Fatal(err)
	}
	future := time.Now().Add(time.Hour)
	r.Usage = usage{Accepted: 1, LastUsedAt: &future, Recent: []time.Time{future}}
	if err := s.write(r); err != nil {
		t.Fatal(err)
	}
	if lease, err := a.Admit(context.Background(), token, "codex/model"); err == nil {
		lease.Close()
		t.Fatal("clock rollback admitted a request")
	}
	info, err = s.Inspect(p.ID)
	if err != nil || info.Accepted != 1 {
		t.Fatalf("rollback changed quota: %+v %v", info, err)
	}
}
