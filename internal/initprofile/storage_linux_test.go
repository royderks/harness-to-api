package initprofile

import "testing"

func TestAtCallRejectsUnsupportedSyscall(t *testing.T) {
	if err := atCall(0, -1, "old", "new", 0); err == nil {
		t.Fatal("accepted unsupported syscall")
	}
}
