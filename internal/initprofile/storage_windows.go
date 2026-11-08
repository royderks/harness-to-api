package initprofile

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"unsafe"
)

const (
	readControl              = 0x00020000
	fileReadAttributes       = 0x80
	ownerSecurityInformation = 1
	daclSecurityInformation  = 4
	seFileObject             = 1
	protectedDACL            = 0x1000
	fileAllAccess            = 0x1f01ff
)

var (
	winAdvapi         = syscall.NewLazyDLL("advapi32.dll")
	winKernel         = syscall.NewLazyDLL("kernel32.dll")
	convertSD         = winAdvapi.NewProc("ConvertStringSecurityDescriptorToSecurityDescriptorW")
	getSecurityInfo   = winAdvapi.NewProc("GetSecurityInfo")
	getSDControl      = winAdvapi.NewProc("GetSecurityDescriptorControl")
	getACLInformation = winAdvapi.NewProc("GetAclInformation")
	getACE            = winAdvapi.NewProc("GetAce")
	equalSID          = winAdvapi.NewProc("EqualSid")
	createDirectory   = winKernel.NewProc("CreateDirectoryW")
	moveFile          = winKernel.NewProc("MoveFileExW")
	getDriveType      = winKernel.NewProc("GetDriveTypeW")
)

type aclInfo struct{ AceCount, AclBytesInUse, AclBytesFree uint32 }
type aceHeader struct {
	Type, Flags uint8
	Size        uint16
}
type accessAllowedACE struct {
	Header   aceHeader
	Mask     uint32
	SidStart uint32
}
type platformStore struct {
	root       *os.Root
	handles    []syscall.Handle
	sid        *syscall.SID
	descriptor uintptr
}

func unsafePath() error { return fmt.Errorf("unsafe onboarding path: use a private local directory") }
func (p platformStore) close() error {
	var first error
	if p.root != nil {
		first = p.root.Close()
	}
	for i := len(p.handles) - 1; i >= 0; i-- {
		if err := syscall.CloseHandle(p.handles[i]); first == nil {
			first = err
		}
	}
	if p.descriptor != 0 {
		if _, err := syscall.LocalFree(syscall.Handle(p.descriptor)); first == nil {
			first = err
		}
	}
	return first
}
func newSecurity() (*syscall.SID, uintptr, error) {
	process, err := syscall.GetCurrentProcess()
	if err != nil {
		return nil, 0, err
	}
	var token syscall.Token
	if err = syscall.OpenProcessToken(process, syscall.TOKEN_QUERY, &token); err != nil {
		return nil, 0, err
	}
	defer token.Close()
	user, err := token.GetTokenUser()
	if err != nil {
		return nil, 0, err
	}
	sid, err := user.User.Sid.Copy()
	if err != nil {
		return nil, 0, err
	}
	text, err := sid.String()
	if err != nil {
		return nil, 0, err
	}
	sddl, err := syscall.UTF16PtrFromString("O:" + text + "D:P(A;;FA;;;" + text + ")")
	if err != nil {
		return nil, 0, err
	}
	var sd uintptr
	r, _, e := convertSD.Call(uintptr(unsafe.Pointer(sddl)), 1, uintptr(unsafe.Pointer(&sd)), 0)
	if r == 0 {
		return nil, 0, e
	}
	return sid, sd, nil
}
func openPrivateState(path string) (string, platformStore, error) {
	if path == "" {
		return "", platformStore{}, unsafePath()
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return "", platformStore{}, err
	}
	volume := filepath.VolumeName(abs)
	if volume == "" || strings.HasPrefix(volume, `\\`) || abs == volume+`\` {
		return "", platformStore{}, unsafePath()
	}
	drive, err := syscall.UTF16PtrFromString(volume + `\`)
	if err != nil {
		return "", platformStore{}, err
	}
	driveType, _, _ := getDriveType.Call(uintptr(unsafe.Pointer(drive)))
	if driveType != 3 {
		return "", platformStore{}, unsafePath()
	}
	sid, sd, err := newSecurity()
	if err != nil {
		return "", platformStore{}, err
	}
	p := platformStore{sid: sid, descriptor: sd}
	ok := false
	defer func() {
		if !ok {
			p.close()
		}
	}()
	current := volume + `\`
	h, err := openDirectoryHandle(current)
	if err != nil {
		return "", platformStore{}, err
	}
	p.handles = append(p.handles, h)
	components := strings.Split(strings.TrimPrefix(abs, current), `\`)
	for i, component := range components {
		current = filepath.Join(current, component)
		if err = p.ensureDirectory(current, i == len(components)-1); err != nil {
			return "", platformStore{}, err
		}
	}
	for _, name := range []string{"keys", "init"} {
		if err = p.ensureDirectory(filepath.Join(abs, name), true); err != nil {
			return "", platformStore{}, err
		}
	}
	if err = p.checkKeyFiles(filepath.Join(abs, "keys")); err != nil {
		return "", platformStore{}, err
	}
	p.root, err = os.OpenRoot(filepath.Join(abs, "init"))
	if err != nil {
		return "", platformStore{}, err
	}
	ok = true
	return filepath.Join(abs, "init"), p, nil
}
func securityAttributes(sd uintptr) syscall.SecurityAttributes {
	return syscall.SecurityAttributes{Length: uint32(unsafe.Sizeof(syscall.SecurityAttributes{})), SecurityDescriptor: sd}
}
func openDirectoryHandle(path string) (syscall.Handle, error) {
	name, err := syscall.UTF16PtrFromString(path)
	if err != nil {
		return 0, err
	}
	h, err := syscall.CreateFile(name, fileReadAttributes|readControl, syscall.FILE_SHARE_READ|syscall.FILE_SHARE_WRITE, nil, syscall.OPEN_EXISTING, syscall.FILE_FLAG_BACKUP_SEMANTICS|syscall.FILE_FLAG_OPEN_REPARSE_POINT, 0)
	if err != nil {
		return 0, err
	}
	var info syscall.ByHandleFileInformation
	if err = syscall.GetFileInformationByHandle(h, &info); err != nil || info.FileAttributes&syscall.FILE_ATTRIBUTE_REPARSE_POINT != 0 || info.FileAttributes&syscall.FILE_ATTRIBUTE_DIRECTORY == 0 {
		syscall.CloseHandle(h)
		if err != nil {
			return 0, err
		}
		return 0, unsafePath()
	}
	return h, nil
}
func (p *platformStore) ensureDirectory(path string, private bool) error {
	h, err := openDirectoryHandle(path)
	if errors.Is(err, syscall.ERROR_FILE_NOT_FOUND) {
		name, nameErr := syscall.UTF16PtrFromString(path)
		if nameErr != nil {
			return nameErr
		}
		sa := securityAttributes(p.descriptor)
		r, _, callErr := createDirectory.Call(uintptr(unsafe.Pointer(name)), uintptr(unsafe.Pointer(&sa)))
		if r == 0 && !errors.Is(callErr, syscall.ERROR_ALREADY_EXISTS) {
			return callErr
		}
		h, err = openDirectoryHandle(path)
	}
	if err != nil {
		return err
	}
	if private {
		if err = p.checkSecurity(h); err != nil {
			syscall.CloseHandle(h)
			return err
		}
	}
	p.handles = append(p.handles, h)
	return nil
}
func (p platformStore) checkKeyFiles(path string) error {
	root, err := os.OpenRoot(path)
	if err != nil {
		return err
	}
	defer root.Close()
	dir, err := root.Open(".")
	if err != nil {
		return err
	}
	defer dir.Close()
	entries, err := dir.ReadDir(-1)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		info, e := root.Lstat(entry.Name())
		if e != nil {
			return e
		}
		if !info.Mode().IsRegular() {
			return unsafePath()
		}
		f, e := root.Open(entry.Name())
		if e != nil {
			return e
		}
		actual, e := f.Stat()
		if e == nil && (!os.SameFile(info, actual) || !actual.Mode().IsRegular()) {
			e = unsafePath()
		}
		var data syscall.ByHandleFileInformation
		if e == nil {
			e = syscall.GetFileInformationByHandle(syscall.Handle(f.Fd()), &data)
		}
		if e == nil && (data.FileAttributes&syscall.FILE_ATTRIBUTE_REPARSE_POINT != 0 || data.NumberOfLinks != 1) {
			e = unsafePath()
		}
		if e == nil {
			e = p.checkOwner(syscall.Handle(f.Fd()))
		}
		f.Close()
		if e != nil {
			return e
		}
	}
	return nil
}
func (p platformStore) checkOwner(h syscall.Handle) error {
	var owner, sd uintptr
	code, _, _ := getSecurityInfo.Call(uintptr(h), seFileObject, ownerSecurityInformation, uintptr(unsafe.Pointer(&owner)), 0, 0, 0, uintptr(unsafe.Pointer(&sd)))
	if code != 0 {
		return syscall.Errno(code)
	}
	defer syscall.LocalFree(syscall.Handle(sd))
	if owner == 0 {
		return unsafePath()
	}
	eq, _, _ := equalSID.Call(owner, uintptr(unsafe.Pointer(p.sid)))
	if eq == 0 {
		return unsafePath()
	}
	return nil
}

func (p platformStore) checkSecurity(h syscall.Handle) error {
	var owner, dacl, sd uintptr
	code, _, _ := getSecurityInfo.Call(uintptr(h), seFileObject, ownerSecurityInformation|daclSecurityInformation, uintptr(unsafe.Pointer(&owner)), 0, uintptr(unsafe.Pointer(&dacl)), 0, uintptr(unsafe.Pointer(&sd)))
	if code != 0 {
		return syscall.Errno(code)
	}
	defer syscall.LocalFree(syscall.Handle(sd))
	if owner == 0 || dacl == 0 {
		return unsafePath()
	}
	eq, _, _ := equalSID.Call(owner, uintptr(unsafe.Pointer(p.sid)))
	if eq == 0 {
		return unsafePath()
	}
	var control uint16
	var revision uint32
	r, _, e := getSDControl.Call(sd, uintptr(unsafe.Pointer(&control)), uintptr(unsafe.Pointer(&revision)))
	if r == 0 {
		return e
	}
	if control&protectedDACL == 0 {
		return unsafePath()
	}
	var info aclInfo
	r, _, e = getACLInformation.Call(dacl, uintptr(unsafe.Pointer(&info)), unsafe.Sizeof(info), 2)
	if r == 0 {
		return e
	}
	if info.AceCount != 1 {
		return unsafePath()
	}
	var ace *accessAllowedACE
	r, _, e = getACE.Call(dacl, 0, uintptr(unsafe.Pointer(&ace)))
	if r == 0 {
		return e
	}
	a := ace
	if a.Header.Type != 0 || a.Header.Flags != 0 || a.Mask != fileAllAccess {
		return unsafePath()
	}
	eq, _, _ = equalSID.Call(uintptr(unsafe.Pointer(&a.SidStart)), uintptr(unsafe.Pointer(p.sid)))
	if eq == 0 {
		return unsafePath()
	}
	return nil
}
func (p platformStore) openRead(_ string, name string) (*os.File, error) {
	info, err := p.root.Lstat(name)
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() {
		return nil, unsafePath()
	}
	f, err := p.root.Open(name)
	if err != nil {
		return nil, err
	}
	actual, err := f.Stat()
	if err == nil && (!os.SameFile(info, actual) || !actual.Mode().IsRegular()) {
		err = unsafePath()
	}
	var data syscall.ByHandleFileInformation
	if err == nil {
		err = syscall.GetFileInformationByHandle(syscall.Handle(f.Fd()), &data)
	}
	if err == nil && (data.FileAttributes&syscall.FILE_ATTRIBUTE_REPARSE_POINT != 0 || data.NumberOfLinks != 1) {
		err = unsafePath()
	}
	if err == nil {
		err = p.checkSecurity(syscall.Handle(f.Fd()))
	}
	if err != nil {
		f.Close()
		return nil, err
	}
	return f, nil
}
func (p platformStore) create(dir, name string) (*os.File, error) {
	path, err := syscall.UTF16PtrFromString(filepath.Join(dir, name))
	if err != nil {
		return nil, err
	}
	sa := securityAttributes(p.descriptor)
	h, err := syscall.CreateFile(path, syscall.GENERIC_WRITE|readControl, syscall.FILE_SHARE_READ, &sa, syscall.CREATE_NEW, syscall.FILE_ATTRIBUTE_NORMAL|syscall.FILE_FLAG_OPEN_REPARSE_POINT, 0)
	if err != nil {
		return nil, err
	}
	if err = p.checkSecurity(h); err != nil {
		syscall.CloseHandle(h)
		p.root.Remove(name)
		return nil, err
	}
	return os.NewFile(uintptr(h), name), nil
}
func (p platformStore) publish(dir, temp, name string, exclusive bool) error {
	if !exclusive {
		f, err := p.openRead(dir, name)
		if err == nil {
			f.Close()
		} else if !errors.Is(err, os.ErrNotExist) {
			return err
		}
	}
	from, err := syscall.UTF16PtrFromString(filepath.Join(dir, temp))
	if err != nil {
		return err
	}
	to, err := syscall.UTF16PtrFromString(filepath.Join(dir, name))
	if err != nil {
		return err
	}
	flags := uintptr(8)
	if !exclusive {
		flags |= 1
	}
	r, _, e := moveFile.Call(uintptr(unsafe.Pointer(from)), uintptr(unsafe.Pointer(to)), flags)
	if r == 0 {
		return e
	}
	return nil
}
func (p platformStore) removeTemp(_, name string) { _ = p.root.Remove(name) }
func (p platformStore) remove(dir, name string) error {
	// A write-through move makes the live name disappear before deletion. The
	// deterministic tombstone is cleaned on a retry after interruption.
	tomb := ".discard-" + name
	old, err := p.openRead(dir, tomb)
	if err == nil {
		old.Close()
		if err = p.root.Remove(tomb); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	f, err := p.openRead(dir, name)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	f.Close()
	from, err := syscall.UTF16PtrFromString(filepath.Join(dir, name))
	if err != nil {
		return err
	}
	to, err := syscall.UTF16PtrFromString(filepath.Join(dir, tomb))
	if err != nil {
		return err
	}
	r, _, e := moveFile.Call(uintptr(unsafe.Pointer(from)), uintptr(unsafe.Pointer(to)), 8)
	if r == 0 {
		return e
	}
	err = p.root.Remove(tomb)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	return err
}
