package initprofile

import (
	"os/exec"
	"testing"
)

func TestDarwinRejectsCredentialACL(t *testing.T) {
	s, _ := newTestStore(t)
	p, token := testProfile(t)
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	if err := s.SaveCredential(token); err != nil {
		t.Fatal(err)
	}
	if out, err := exec.Command("chmod", "+a", "everyone allow read", s.CredentialPath()).CombinedOutput(); err != nil {
		t.Fatalf("set ACL: %v: %s", err, out)
	}
	if _, _, err := s.Load(); err == nil {
		t.Fatal("accepted credential with extended ACL")
	}
}
