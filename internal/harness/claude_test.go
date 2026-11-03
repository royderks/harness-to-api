package harness

import (
	"context"
	"errors"
	"os/exec"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func TestClaudeFixture(t *testing.T) {
	binary := filepath.Join(t.TempDir(), "claude-fixture")
	if runtime.GOOS == "windows" {
		binary += ".exe"
	}
	cmd := exec.Command(filepath.Join(runtime.GOROOT(), "bin", "go"), "build", "-o", binary, "./testdata/claude-cli")
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build fixture: %v: %s", err, output)
	}
	t.Setenv("OPENAI_API_KEY", "must-not-forward")
	t.Setenv("CLAUDE_CODE_OAUTH_TOKEN", "must-not-forward")
	c := Claude{Binary: binary}
	if c.ID() != "claude" {
		t.Fatal(c.ID())
	}
	run := func(ctx context.Context, prompt string, emit func(string) error) (*Usage, error) {
		return c.Run(ctx, Request{Model: "sonnet", Prompt: prompt}, emit)
	}
	t.Run("text and usage", func(t *testing.T) {
		var text string
		u, err := run(context.Background(), "ok", func(s string) error { text += s; return nil })
		if err != nil || text != "Hello" || u == nil || *u != (Usage{PromptTokens: 14, CompletionTokens: 3, TotalTokens: 17}) {
			t.Fatalf("text=%q usage=%+v err=%v", text, u, err)
		}
	})
	for _, prompt := range []string{"bad", "incomplete", "error", "auth", "tool", "after"} {
		t.Run(prompt, func(t *testing.T) {
			if _, err := run(context.Background(), prompt, func(string) error { return nil }); err == nil {
				t.Fatal("expected error")
			}
		})
	}
	t.Run("emit error", func(t *testing.T) {
		sentinel := errors.New("emit failed")
		_, err := run(context.Background(), "ok", func(string) error { return sentinel })
		if !errors.Is(err, sentinel) {
			t.Fatal(err)
		}
	})
	t.Run("cancellation", func(t *testing.T) {
		ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
		defer cancel()
		_, err := run(ctx, "cancel", func(string) error { return nil })
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatal(err)
		}
	})
}
