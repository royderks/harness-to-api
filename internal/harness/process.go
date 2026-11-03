package harness

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

func childEnvironment() []string {
	names := []string{"PATH", "HOME", "USERPROFILE", "SystemRoot", "COMSPEC", "PATHEXT", "TMPDIR", "TMP", "TEMP", "LANG", "LC_ALL", "CODEX_HOME", "XDG_CONFIG_HOME", "HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY", "SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS"}
	env := []string{}
	for _, name := range names {
		if value, ok := os.LookupEnv(name); ok {
			env = append(env, name+"="+value)
		}
	}
	return env
}
func runLines(ctx context.Context, binary string, args []string, input string, consume func([]byte) error) error {
	binary, err := exec.LookPath(binary)
	if err != nil {
		return fmt.Errorf("find harness: %w", err)
	}
	binary, err = filepath.Abs(binary)
	if err != nil {
		return err
	}
	directory, err := os.MkdirTemp("", "h2a-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(directory)
	cmd := exec.CommandContext(ctx, binary, args...)
	cmd.Dir = directory
	cmd.Env = childEnvironment()
	cmd.Stdin = strings.NewReader(input)
	cmd.Stderr = io.Discard
	cmd.WaitDelay = 2 * time.Second
	configureProcess(cmd)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	if err = cmd.Start(); err != nil {
		return fmt.Errorf("start harness: %w", err)
	}
	scanner := bufio.NewScanner(stdout)
	scanner.Buffer(make([]byte, 64*1024), 8*1024*1024)
	total := 0
	for scanner.Scan() {
		total += len(scanner.Bytes())
		if total > 16*1024*1024 {
			err = errors.New("harness output exceeded 16 MiB")
			break
		}
		if len(scanner.Bytes()) == 0 {
			continue
		}
		if err = consume(scanner.Bytes()); err != nil {
			break
		}
	}
	if err == nil {
		err = scanner.Err()
	}
	if err != nil {
		_ = cmd.Cancel()
	}
	waitErr := cmd.Wait()
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if err != nil {
		return err
	}
	if waitErr != nil {
		return fmt.Errorf("harness process failed: %w", waitErr)
	}
	return nil
}
