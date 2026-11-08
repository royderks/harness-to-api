package initprofile

import (
	"encoding/binary"
	"os"
	"syscall"
	"unsafe"
)

// Darwin's syscall package omits these public *at syscall numbers.
func fileExclusiveAt(fd int, old, new string) error { return atCall(488, fd, old, new, 4) }
func fileRenameAt(fd int, old, new string) error    { return atCall(465, fd, old, new, 0) }

func checkLocalFilesystem(dir *os.File) error {
	var info syscall.Statfs_t
	if err := syscall.Fstatfs(int(dir.Fd()), &info); err != nil {
		return err
	}
	if info.Flags&0x1000 == 0 {
		return unsafePath()
	}
	return nil
}

// macOS ACLs can grant rights independently of the mode bits. Refuse any
// extended ACL on private objects; inherited permissions must never expose a
// credential before it is written.
func checkFileSecurity(f *os.File) error {
	attrs := [6]uint32{5, 0x00400000}
	var b [8192]byte
	_, _, errno := syscall.Syscall6(228, f.Fd(), uintptr(unsafe.Pointer(&attrs[0])), uintptr(unsafe.Pointer(&b[0])), uintptr(len(b)), 0, 0)
	if errno != 0 {
		return errno
	}
	size := binary.LittleEndian.Uint32(b[:4])
	if size < 12 || size > uint32(len(b)) {
		return unsafePath()
	}
	// attrreference begins after the returned size; a zero length means no ACL.
	if binary.LittleEndian.Uint32(b[8:12]) != 0 {
		return unsafePath()
	}
	return nil
}
