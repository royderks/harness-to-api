package auth

import (
	"os"
	"path/filepath"
	"syscall"
	"unsafe"
)

var (
	kernel32     = syscall.NewLazyDLL("kernel32.dll")
	lockFileEx   = kernel32.NewProc("LockFileEx")
	unlockFileEx = kernel32.NewProc("UnlockFileEx")
	moveFileEx   = kernel32.NewProc("MoveFileExW")
)

func lockFile(path string, wait bool) (*os.File, error) {
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	flags := uintptr(2) // LOCKFILE_EXCLUSIVE_LOCK
	if !wait {
		flags |= 1
	} // LOCKFILE_FAIL_IMMEDIATELY
	var overlap syscall.Overlapped
	r, _, callErr := lockFileEx.Call(f.Fd(), flags, 0, 1, 0, uintptr(unsafe.Pointer(&overlap)))
	if r == 0 {
		f.Close()
		return nil, os.NewSyscallError("LockFileEx", callErr)
	}
	return f, nil
}

func unlockFile(f *os.File) {
	var overlap syscall.Overlapped
	unlockFileEx.Call(f.Fd(), 0, 1, 0, uintptr(unsafe.Pointer(&overlap)))
	f.Close()
}

// The source is already flushed, and both names are in the same directory.
// Windows has no portable equivalent of fsync on a directory; use the
// documented write-through move and do not claim POSIX directory-fsync semantics.
func replaceRecord(src, dst string) error {
	from, err := syscall.UTF16PtrFromString(src)
	if err != nil {
		return err
	}
	to, err := syscall.UTF16PtrFromString(dst)
	if err != nil {
		return err
	}
	const replaceExisting = 1
	const writeThrough = 8
	r, _, callErr := moveFileEx.Call(uintptr(unsafe.Pointer(from)), uintptr(unsafe.Pointer(to)), replaceExisting|writeThrough)
	if r == 0 {
		return &os.LinkError{Op: "MoveFileEx", Old: src, New: dst, Err: callErr}
	}
	return nil
}

func removeRecord(path string) error {
	// A crash after the move leaves an ignored tombstone, not a live key.
	f, err := os.CreateTemp(filepath.Dir(path), ".revoked-")
	if err != nil {
		return err
	}
	name := f.Name()
	if err = f.Close(); err != nil {
		os.Remove(name)
		return err
	}
	if err = replaceRecord(path, name); err != nil {
		os.Remove(name)
		return err
	}
	return os.Remove(name)
}
