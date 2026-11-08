//go:build linux || darwin

package initprofile

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRejectUnsafeUnixPaths(t *testing.T) {
	parent, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Run("state mode", func(t *testing.T) {
		path := filepath.Join(parent, "permissive")
		if err := os.Mkdir(path, 0755); err != nil {
			t.Fatal(err)
		}
		if s, err := Open(path); err == nil {
			s.Close()
			t.Fatal("accepted public state")
		}
	})
	t.Run("state symlink", func(t *testing.T) {
		target := filepath.Join(parent, "target")
		if err := os.Mkdir(target, 0700); err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(parent, "link")
		if err := os.Symlink(target, path); err != nil {
			t.Fatal(err)
		}
		if s, err := Open(path); err == nil {
			s.Close()
			t.Fatal("accepted symlink")
		}
	})
	t.Run("key symlink", func(t *testing.T) {
		state := filepath.Join(parent, "keys-link")
		if err := os.Mkdir(state, 0700); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(parent, filepath.Join(state, "keys")); err != nil {
			t.Fatal(err)
		}
		if s, err := Open(state); err == nil {
			s.Close()
			t.Fatal("accepted key symlink")
		}
	})
}
func TestRejectUnsafeUnixCredential(t *testing.T) {
	for _, kind := range []string{"mode", "symlink", "hardlink", "directory"} {
		t.Run(kind, func(t *testing.T) {
			s, _ := newTestStore(t)
			p, token := testProfile(t)
			if err := s.SaveProfile(p); err != nil {
				t.Fatal(err)
			}
			path := s.CredentialPath()
			switch kind {
			case "mode":
				if err := os.WriteFile(path, []byte(token), 0644); err != nil {
					t.Fatal(err)
				}
			case "symlink":
				if err := os.Symlink(filepath.Join(t.TempDir(), "missing"), path); err != nil {
					t.Fatal(err)
				}
			case "hardlink":
				other := filepath.Join(s.dir, "other")
				if err := os.WriteFile(other, []byte(token), 0600); err != nil {
					t.Fatal(err)
				}
				if err := os.Link(other, path); err != nil {
					t.Fatal(err)
				}
			case "directory":
				if err := os.Mkdir(path, 0700); err != nil {
					t.Fatal(err)
				}
			}
			if _, _, err := s.Load(); err == nil {
				t.Fatal("accepted unsafe credential")
			}
			if err := s.RemoveCredential(); err == nil {
				t.Fatal("removed unsafe credential")
			}
		})
	}
}

func TestRejectKeyRecordSymlinksBeforeAuth(t *testing.T) {
	state := filepath.Join(t.TempDir(), "state")
	s, err := Open(state)
	if err != nil {
		t.Fatal(err)
	}
	if err = s.Close(); err != nil {
		t.Fatal(err)
	}
	if err = os.Symlink(filepath.Join(t.TempDir(), "target"), filepath.Join(state, "keys", ".server.lock")); err != nil {
		t.Fatal(err)
	}
	if s, err = Open(state); err == nil {
		s.Close()
		t.Fatal("accepted linked lock file")
	}
}
