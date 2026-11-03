package harness

import (
	"context"
	"errors"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func openCodeFixture(t *testing.T) string {
	t.Helper()
	binary := filepath.Join(t.TempDir(), "opencode")
	if runtime.GOOS == "windows" {
		binary += ".exe"
	}
	cmd := exec.Command(filepath.Join(runtime.GOROOT(), "bin", "go"), "build", "-o", binary, "./testdata/opencode-cli")
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build fixture: %v: %s", err, output)
	}
	return binary
}

func TestOpenCode(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "gateway-secret")
	adapter := OpenCode{Binary: openCodeFixture(t)}
	if adapter.ID() != "opencode" {
		t.Fatal(adapter.ID())
	}
	var out strings.Builder
	usage, err := adapter.Run(context.Background(), Request{Model: "anthropic/claude/example", Prompt: "good"}, func(s string) error { _, err := out.WriteString(s); return err })
	if err != nil || out.String() != "hello" || usage == nil || *usage != (Usage{PromptTokens: 15, CompletionTokens: 5, TotalTokens: 20}) {
		t.Fatalf("text=%q usage=%+v err=%v", out.String(), usage, err)
	}
	for _, prompt := range []string{"error", "incomplete", "malformed", "tool", "badfinish"} {
		t.Run(prompt, func(t *testing.T) {
			_, err := adapter.Run(context.Background(), Request{Model: "default", Prompt: prompt}, func(string) error { return nil })
			if err == nil {
				t.Fatal("expected error")
			}
		})
	}
	t.Run("cancel", func(t *testing.T) {
		ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
		defer cancel()
		_, err := adapter.Run(ctx, Request{Model: "default", Prompt: "cancel"}, func(string) error { return nil })
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatal(err)
		}
	})
	t.Run("emit error", func(t *testing.T) {
		want := errors.New("emit failed")
		_, err := adapter.Run(context.Background(), Request{Model: "default", Prompt: "emit"}, func(string) error { return want })
		if !errors.Is(err, want) {
			t.Fatal(err)
		}
	})
}
