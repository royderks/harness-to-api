package harness

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func TestGeminiFixture(t *testing.T) {
	binary := filepath.Join(t.TempDir(), "gemini-fixture")
	if runtime.GOOS == "windows" {
		binary += ".exe"
	}
	goBinary := filepath.Join(runtime.GOROOT(), "bin", "go")
	if runtime.GOOS == "windows" {
		goBinary += ".exe"
	}
	cmd := exec.Command(goBinary, "build", "-o", binary, "./testdata/gemini-cli")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build fixture: %v: %s", err, out)
	}
	t.Setenv("OPENAI_API_KEY", "must-not-leak")
	t.Setenv("CODEX_HOME", "must-not-leak")
	t.Setenv("GOOGLE_CLOUD_PROJECT", "fixture-project")
	t.Setenv("GEMINI_API_KEY", "fixture-key")
	adapter := Gemini{Binary: binary}
	if adapter.ID() != "gemini" {
		t.Fatal("wrong ID")
	}
	t.Run("text and usage", func(t *testing.T) {
		var out strings.Builder
		usage, err := adapter.Run(context.Background(), Request{Model: "default", Prompt: "valid"}, func(s string) error { _, err := out.WriteString(s); return err })
		if err != nil || out.String() != "hello world" || usage == nil || *usage != (Usage{PromptTokens: 12, CompletionTokens: 3, TotalTokens: 16}) {
			t.Fatalf("text %q usage %+v err %v", out.String(), usage, err)
		}
	})
	t.Run("model", func(t *testing.T) {
		_, err := adapter.Run(context.Background(), Request{Model: "gemini-test", Prompt: "model"}, func(string) error { return nil })
		if err != nil {
			t.Fatal(err)
		}
	})
	for _, scenario := range []string{"error", "error-event", "incomplete", "malformed", "tool"} {
		t.Run(scenario, func(t *testing.T) {
			if _, err := adapter.Run(context.Background(), Request{Prompt: scenario}, func(string) error { return nil }); err == nil {
				t.Fatal("expected error")
			}
		})
	}
	t.Run("cancellation", func(t *testing.T) {
		ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
		defer cancel()
		_, err := adapter.Run(ctx, Request{Prompt: "cancel"}, func(string) error { return nil })
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("%v", err)
		}
	})
	t.Run("emit error", func(t *testing.T) {
		sentinel := errors.New("emit failed")
		_, err := adapter.Run(context.Background(), Request{Prompt: "emit"}, func(string) error { return sentinel })
		if !errors.Is(err, sentinel) {
			t.Fatalf("%v", err)
		}
	})
}

func TestGeminiLive(t *testing.T) {
	if os.Getenv("H2A_GEMINI_LIVE") != "1" {
		t.Skip("opt-in live request")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()
	var out strings.Builder
	usage, err := (Gemini{Binary: os.Getenv("H2A_GEMINI_BIN")}).Run(ctx, Request{Model: "default", Prompt: "Reply with exactly: hello"}, func(s string) error { _, err := out.WriteString(s); return err })
	if err != nil {
		t.Fatal(err)
	}
	if strings.TrimSpace(out.String()) == "" {
		t.Fatal("empty response")
	}
	t.Logf("received text and usage=%v", usage != nil)
}
