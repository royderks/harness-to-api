//go:build linux || darwin

package initprofile

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"syscall"
	"unsafe"
)

type platformStore struct {
	root *os.Root
	dir  *os.File
}

func (p platformStore) close() error {
	e := p.dir.Close()
	if err := p.root.Close(); e == nil {
		e = err
	}
	return e
}
func unsafePath() error { return fmt.Errorf("unsafe onboarding path: use a private local directory") }

// Canonicalize only the operating system's root-owned aliases. Other symlinks
// are refused, including symlinks to state, keys, and init.
func systemAliases(path string) (string, error) {
	if runtime.GOOS != "darwin" {
		return path, nil
	}
	for _, pair := range [][2]string{{"/tmp", "/private/tmp"}, {"/var", "/private/var"}} {
		if path == pair[0] || strings.HasPrefix(path, pair[0]+"/") {
			info, err := os.Lstat(pair[0])
			if err != nil {
				return "", err
			}
			st, ok := info.Sys().(*syscall.Stat_t)
			if !ok || st.Uid != 0 || info.Mode()&os.ModeSymlink == 0 {
				return "", unsafePath()
			}
			target, err := os.Readlink(pair[0])
			if err != nil || target != strings.TrimPrefix(pair[1], "/") {
				return "", unsafePath()
			}
			return pair[1] + strings.TrimPrefix(path, pair[0]), nil
		}
	}
	return path, nil
}
func openPrivateState(path string) (string, platformStore, error) {
	if path == "" {
		return "", platformStore{}, unsafePath()
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return "", platformStore{}, err
	}
	abs, err = systemAliases(abs)
	if err != nil {
		return "", platformStore{}, err
	}
	if abs == "/" {
		return "", platformStore{}, unsafePath()
	}
	current, err := os.OpenRoot("/")
	if err != nil {
		return "", platformStore{}, err
	}
	defer func() {
		if current != nil {
			current.Close()
		}
	}()
	parts := strings.Split(strings.TrimPrefix(abs, "/"), "/")
	for i, name := range parts {
		next, e := openDirectory(current, name, i == len(parts)-1)
		if e != nil {
			return "", platformStore{}, e
		}
		current.Close()
		current = next
	}
	for _, name := range []string{"keys", "init"} {
		child, e := openDirectory(current, name, true)
		if e != nil {
			return "", platformStore{}, e
		}
		dir, e := child.Open(".")
		if e == nil {
			e = checkLocalFilesystem(dir)
		}
		if e == nil && name == "keys" {
			e = checkKeyFiles(child, dir)
		}
		if dir != nil {
			dir.Close()
		}
		child.Close()
		if e != nil {
			return "", platformStore{}, e
		}
	}
	initRoot, err := current.OpenRoot("init")
	if err != nil {
		return "", platformStore{}, err
	}
	info, err := initRoot.Stat(".")
	if err != nil || checkDirectory(info, true) != nil {
		initRoot.Close()
		return "", platformStore{}, unsafePath()
	}
	dir, err := initRoot.Open(".")
	if err != nil {
		initRoot.Close()
		return "", platformStore{}, err
	}
	return filepath.Join(abs, "init"), platformStore{initRoot, dir}, nil
}
func openDirectory(parent *os.Root, name string, private bool) (*os.Root, error) {
	info, err := parent.Lstat(name)
	if errors.Is(err, os.ErrNotExist) {
		if err = parent.Mkdir(name, 0700); err != nil && !errors.Is(err, os.ErrExist) {
			return nil, err
		}
		info, err = parent.Lstat(name)
	}
	if err != nil {
		return nil, err
	}
	if err = checkDirectory(info, private); err != nil {
		return nil, err
	}
	root, err := parent.OpenRoot(name)
	if err != nil {
		return nil, err
	}
	actual, err := root.Stat(".")
	if err == nil {
		err = checkDirectory(actual, private)
	}
	if err == nil && !os.SameFile(info, actual) {
		err = unsafePath()
	}
	if err == nil && private {
		var f *os.File
		f, err = root.Open(".")
		if err == nil {
			err = checkFileSecurity(f)
			f.Close()
		}
	}
	if err != nil {
		root.Close()
		return nil, err
	}
	return root, nil
}
func checkKeyFiles(root *os.Root, dir *os.File) error {
	entries, err := dir.ReadDir(-1)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		info, e := root.Lstat(entry.Name())
		if e != nil {
			return e
		}
		if e = checkFile(info); e != nil {
			return e
		}
		f, e := root.OpenFile(entry.Name(), os.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
		if e != nil {
			return e
		}
		actual, e := f.Stat()
		if e == nil && !os.SameFile(info, actual) {
			e = unsafePath()
		}
		if e == nil {
			e = checkFileSecurity(f)
		}
		f.Close()
		if e != nil {
			return e
		}
	}
	return nil
}
func checkDirectory(info os.FileInfo, private bool) error {
	if info == nil {
		return unsafePath()
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return unsafePath()
	}
	uid := uint32(os.Geteuid())
	if private {
		if stat.Uid != uid || info.Mode().Perm() != 0700 || info.Mode()&(os.ModeSetuid|os.ModeSetgid|os.ModeSticky) != 0 {
			return unsafePath()
		}
	} else {
		if stat.Uid != 0 && stat.Uid != uid {
			return unsafePath()
		}
		if info.Mode().Perm()&0022 != 0 && !(stat.Uid == 0 && info.Mode()&os.ModeSticky != 0) {
			return unsafePath()
		}
	}
	return nil
}
func checkFile(info os.FileInfo) error {
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || !info.Mode().IsRegular() || stat.Uid != uint32(os.Geteuid()) || stat.Nlink != 1 || info.Mode().Perm() != 0600 || info.Mode()&(os.ModeSetuid|os.ModeSetgid|os.ModeSticky) != 0 {
		return unsafePath()
	}
	return nil
}
func (p platformStore) openRead(_ string, name string) (*os.File, error) {
	info, err := p.root.Lstat(name)
	if err != nil {
		return nil, err
	}
	if err = checkFile(info); err != nil {
		return nil, err
	}
	f, err := p.root.OpenFile(name, os.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if err != nil {
		return nil, err
	}
	actual, err := f.Stat()
	if err == nil {
		err = checkFile(actual)
	}
	if err == nil && !os.SameFile(info, actual) {
		err = unsafePath()
	}
	if err == nil {
		err = checkFileSecurity(f)
	}
	if err != nil {
		f.Close()
		return nil, err
	}
	return f, nil
}
func (p platformStore) create(_ string, name string) (*os.File, error) {
	f, err := p.root.OpenFile(name, os.O_CREATE|os.O_EXCL|os.O_WRONLY|syscall.O_NOFOLLOW, 0600)
	if err != nil {
		return nil, err
	}
	info, err := f.Stat()
	if err == nil {
		err = checkFile(info)
	}
	if err == nil {
		err = checkFileSecurity(f)
	}
	if err != nil {
		f.Close()
		p.root.Remove(name)
		return nil, err
	}
	return f, nil
}
func (p platformStore) publish(_ string, temp, name string, exclusive bool) error {
	if exclusive {
		if err := fileExclusiveAt(int(p.dir.Fd()), temp, name); err != nil {
			return err
		}
	} else {
		f, err := p.openRead("", name)
		if err == nil {
			f.Close()
		} else if !errors.Is(err, os.ErrNotExist) {
			return err
		}
		if err = fileRenameAt(int(p.dir.Fd()), temp, name); err != nil {
			return err
		}
	}
	return p.dir.Sync()
}
func (p platformStore) removeTemp(_ string, name string) { _ = p.root.Remove(name) }
func (p platformStore) remove(_ string, name string) error {
	f, err := p.openRead("", name)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	f.Close()
	if err = p.root.Remove(name); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return p.dir.Sync()
}
func atCall(trap uintptr, fd int, old, new string, flags uintptr) error {
	a, err := syscall.BytePtrFromString(old)
	if err != nil {
		return err
	}
	b, err := syscall.BytePtrFromString(new)
	if err != nil {
		return err
	}
	_, _, errno := syscall.Syscall6(trap, uintptr(fd), uintptr(unsafe.Pointer(a)), uintptr(fd), uintptr(unsafe.Pointer(b)), flags, 0)
	if errno != 0 {
		return errno
	}
	return nil
}
