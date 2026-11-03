//go:build !linux && !darwin && !windows

package auth

import (
	"errors"
	"os"
)

func lockFile(string, bool) (*os.File, error) {
	return nil, errors.New("auth file locking unsupported on this platform")
}
func unlockFile(f *os.File) { f.Close() }
func syncDir(string) error  { return errors.New("auth directory sync unsupported on this platform") }

func replaceRecord(string, string) error {
	return errors.New("auth record replacement unsupported on this platform")
}
func removeRecord(string) error {
	return errors.New("auth record deletion unsupported on this platform")
}
