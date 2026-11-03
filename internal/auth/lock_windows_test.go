package auth

import (
	"path/filepath"
	"testing"
	"time"
)

func TestWindowsLockWaitAndRelease(t *testing.T) {
	path := filepath.Join(t.TempDir(), "lock")
	first, err := lockFile(path, true)
	if err != nil {
		t.Fatal(err)
	}
	if other, err := lockFile(path, false); err == nil {
		unlockFile(other)
		unlockFile(first)
		t.Fatal("nonblocking lock succeeded while held")
	}
	acquired := make(chan error, 1)
	go func() {
		f, err := lockFile(path, true)
		if err == nil {
			unlockFile(f)
		}
		acquired <- err
	}()
	select {
	case err := <-acquired:
		unlockFile(first)
		t.Fatalf("waiting lock returned early: %v", err)
	case <-time.After(100 * time.Millisecond):
	}
	unlockFile(first)
	select {
	case err := <-acquired:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("waiting lock did not acquire after release")
	}
}
