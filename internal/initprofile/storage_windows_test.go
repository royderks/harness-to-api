package initprofile

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"
	"unsafe"
)

func publicDescriptor(t *testing.T) uintptr {
	t.Helper()
	sddl, err := syscall.UTF16PtrFromString("D:P(A;;FA;;;WD)")
	if err != nil {
		t.Fatal(err)
	}
	var sd uintptr
	r, _, e := convertSD.Call(uintptr(unsafe.Pointer(sddl)), 1, uintptr(unsafe.Pointer(&sd)), 0)
	if r == 0 {
		t.Fatal(e)
	}
	t.Cleanup(func() { syscall.LocalFree(syscall.Handle(sd)) })
	return sd
}
func TestWindowsRejectsPublicStateDACL(t *testing.T) {
	path := filepath.Join(t.TempDir(), "public")
	name, err := syscall.UTF16PtrFromString(path)
	if err != nil {
		t.Fatal(err)
	}
	sa := securityAttributes(publicDescriptor(t))
	r, _, e := createDirectory.Call(uintptr(unsafe.Pointer(name)), uintptr(unsafe.Pointer(&sa)))
	if r == 0 {
		t.Fatal(e)
	}
	if s, err := Open(path); err == nil {
		s.Close()
		t.Fatal("accepted public DACL")
	}
}
func TestWindowsRejectsPublicCredentialDACL(t *testing.T) {
	s, _ := newTestStore(t)
	profile, token := testProfile(t)
	if err := s.SaveProfile(profile); err != nil {
		t.Fatal(err)
	}
	name, err := syscall.UTF16PtrFromString(s.CredentialPath())
	if err != nil {
		t.Fatal(err)
	}
	sa := securityAttributes(publicDescriptor(t))
	h, err := syscall.CreateFile(name, syscall.GENERIC_WRITE, syscall.FILE_SHARE_READ, &sa, syscall.CREATE_NEW, syscall.FILE_ATTRIBUTE_NORMAL, 0)
	if err != nil {
		t.Fatal(err)
	}
	f := os.NewFile(uintptr(h), s.CredentialPath())
	if _, err = f.WriteString(token); err != nil {
		t.Fatal(err)
	}
	if err = f.Close(); err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.Load(); err == nil {
		t.Fatal("accepted public credential")
	}
}

func TestWindowsRejectsLinkedState(t *testing.T) {
	parent := t.TempDir()
	target := filepath.Join(parent, "target")
	if err := os.Mkdir(target, 0700); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(parent, "linked")
	if err := os.Symlink(target, link); err != nil {
		t.Skipf("symlink creation unavailable: %v", err)
	}
	if s, err := Open(link); err == nil {
		s.Close()
		t.Fatal("accepted reparse point")
	}
}

func TestWindowsRemoveRetriesInterruptedMove(t *testing.T) {
	s, _ := newTestStore(t)
	p, token := testProfile(t)
	if err := s.SaveProfile(p); err != nil {
		t.Fatal(err)
	}
	if err := s.SaveCredential(token); err != nil {
		t.Fatal(err)
	}
	from, err := syscall.UTF16PtrFromString(s.CredentialPath())
	if err != nil {
		t.Fatal(err)
	}
	to, err := syscall.UTF16PtrFromString(filepath.Join(s.dir, ".discard-client.key"))
	if err != nil {
		t.Fatal(err)
	}
	r, _, e := moveFile.Call(uintptr(unsafe.Pointer(from)), uintptr(unsafe.Pointer(to)), 8)
	if r == 0 {
		t.Fatal(e)
	}
	if err = s.RemoveCredential(); err != nil {
		t.Fatal(err)
	}
	if _, err = os.Stat(filepath.Join(s.dir, ".discard-client.key")); !os.IsNotExist(err) {
		t.Fatalf("tombstone remains: %v", err)
	}
}
