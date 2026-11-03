package harness

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

var fixture string

func TestMain(m *testing.M) {
	directory, err := os.MkdirTemp("", "h2a-fixture-")
	if err != nil {
		panic(err)
	}
	fixture = filepath.Join(directory, "fake-cli")
	if runtime.GOOS == "windows" {
		fixture += ".exe"
	}
	goBinary := filepath.Join(runtime.GOROOT(), "bin", "go")
	cmd := exec.Command(goBinary, "build", "-o", fixture, "./testdata/fake-cli")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err = cmd.Run(); err != nil {
		os.RemoveAll(directory)
		panic(err)
	}
	code := m.Run()
	os.RemoveAll(directory)
	os.Exit(code)
}
func TestOfficialProtocols(t *testing.T) {
	t.Setenv("HARNESS_API_KEY", "must-not-reach-child")
	for _, a := range []Adapter{Codex{Binary: fixture}, Antigravity{Binary: fixture}} {
		t.Run(a.ID(), func(t *testing.T) {
			var text strings.Builder
			ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
			defer cancel()
			usage, err := a.Run(ctx, Request{Model: "default", Prompt: "quotes \" \n $(touch injected)"}, func(delta string) error { text.WriteString(delta); return nil })
			if err != nil {
				t.Fatal(err)
			}
			if text.String() != "Hello 🌍" || usage == nil || usage.TotalTokens != 7 {
				t.Fatalf("text=%q usage=%+v", text.String(), usage)
			}
		})
	}
}
func TestProcessFailures(t *testing.T) {
	for _, a := range []Adapter{Codex{Binary: fixture}, Antigravity{Binary: fixture}} {
		for _, mode := range []string{"missing-result", "bad-json", "exit-error", "hang"} {
			t.Run(a.ID()+"/"+mode, func(t *testing.T) {
				ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
				defer cancel()
				_, err := a.Run(ctx, Request{Model: mode, Prompt: "hi"}, func(string) error { return nil })
				if err == nil {
					t.Fatal("expected failure")
				}
				if strings.Contains(err.Error(), "sensitive") {
					t.Fatal("stderr leaked")
				}
			})
		}
	}
}
func TestMissingBinary(t *testing.T) {
	_, err := (Antigravity{Binary: filepath.Join(t.TempDir(), "does-not-exist")}).Run(context.Background(), Request{Model: "default", Prompt: "hi"}, func(string) error { return nil })
	if err == nil {
		t.Fatal("missing binary succeeded")
	}
}
func TestLiveCodex(t *testing.T) {
	if os.Getenv("H2A_LIVE_CODEX") != "1" {
		t.Skip("set H2A_LIVE_CODEX=1 to make a real request using the local Codex login")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	var text strings.Builder
	_, err := (Codex{}).Run(ctx, Request{Model: "default", Prompt: "Reply with exactly H2A_OK. Do not use tools."}, func(delta string) error { text.WriteString(delta); return nil })
	if err != nil {
		t.Fatal(err)
	}
	if strings.TrimSpace(text.String()) != "H2A_OK" {
		t.Fatalf("unexpected response %q", text.String())
	}
}
