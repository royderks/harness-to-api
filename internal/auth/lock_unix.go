//go:build linux || darwin

package auth

import (
	"os"
	"path/filepath"
	"syscall"
)

func lockFile(path string, wait bool) (*os.File, error) {
	f, e := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
	if e != nil {
		return nil, e
	}
	op := syscall.LOCK_EX
	if !wait {
		op |= syscall.LOCK_NB
	}
	if e = syscall.Flock(int(f.Fd()), op); e != nil {
		f.Close()
		return nil, e
	}
	return f, nil
}
func unlockFile(f *os.File) { syscall.Flock(int(f.Fd()), syscall.LOCK_UN); f.Close() }
func syncDir(path string) error {
	f, e := os.Open(path)
	if e != nil {
		return e
	}
	defer f.Close()
	return f.Sync()
}

func replaceRecord(src, dst string) error {
	if err := os.Rename(src, dst); err != nil {
		return err
	}
	return syncDir(filepath.Dir(dst))
}
func removeRecord(path string) error {
	if err := os.Remove(path); err != nil {
		return err
	}
	return syncDir(filepath.Dir(path))
}
