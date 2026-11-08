//go:build linux && !amd64 && !arm64

package initprofile

// Unsupported architectures fail closed rather than using an unknown syscall.
const renameat2Number = 0
