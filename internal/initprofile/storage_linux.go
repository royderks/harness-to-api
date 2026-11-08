package initprofile

import (
	"os"
	"syscall"
)

func fileExclusiveAt(fd int, old, new string) error { return atCall(renameat2Number, fd, old, new, 1) }
func fileRenameAt(fd int, old, new string) error {
	return atCall(syscall.SYS_RENAMEAT, fd, old, new, 0)
}

func checkLocalFilesystem(dir *os.File) error {
	var info syscall.Statfs_t
	if err := syscall.Fstatfs(int(dir.Fd()), &info); err != nil {
		return err
	}
	switch uint64(info.Type) {
	case 0xef53, 0x58465342, 0x9123683e, 0x01021994, 0x794c7630, 0x2fc12fc1, 0x858458f6:
		return nil
	default:
		return unsafePath()
	}
}

// Linux's mode bits include the effective mask for a POSIX access ACL.
func checkFileSecurity(*os.File) error { return nil }
