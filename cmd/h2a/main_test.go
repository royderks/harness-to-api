package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestCLIHelper(t *testing.T) {
	if os.Getenv("H2A_TEST_HELPER") != "1" {
		return
	}
	for i, arg := range os.Args {
		if arg == "--" {
			if err := run(os.Args[i+1:]); err != nil {
				// Match main's treatment of flag help.
				if !errors.Is(err, flag.ErrHelp) {
					var failure *initFailure
					if errors.As(err, &failure) {
						os.Exit(failure.status)
					}
					fmt.Fprintln(os.Stderr, "h2a:", err)
					os.Exit(1)
				}
			}
			os.Exit(0)
		}
	}
	os.Exit(2)
}

func cli(t *testing.T, args ...string) (string, string, error) {
	t.Helper()
	cmd := exec.Command(os.Args[0], append([]string{"-test.run=^TestCLIHelper$", "--"}, args...)...)
	cmd.Env = append(os.Environ(), "H2A_TEST_HELPER=1")
	var stdout, stderr strings.Builder
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}

func TestKeyCommands(t *testing.T) {
	state := t.TempDir()
	out, diagnostics, err := cli(t, "keys", "create", "--state-dir", state, "--name", "demo", "--models", "codex/*")
	if err != nil {
		t.Fatalf("create: %v: %s", err, diagnostics)
	}
	token := strings.TrimSpace(out)
	if !strings.HasPrefix(token, "h2a_") || strings.Contains(diagnostics, token) {
		t.Fatalf("unexpected create output: %q %q", out, diagnostics)
	}
	out, diagnostics, err = cli(t, "keys", "list", "--state-dir", state)
	if err != nil {
		t.Fatalf("list: %v: %s", err, diagnostics)
	}
	var keys []struct {
		ID     string   `json:"id"`
		Name   string   `json:"name"`
		Models []string `json:"models"`
	}
	if err := json.Unmarshal([]byte(out), &keys); err != nil || len(keys) != 1 || keys[0].Name != "demo" || len(keys[0].Models) != 1 || keys[0].Models[0] != "codex/*" {
		t.Fatalf("list: %q: %v", out, err)
	}
	data, err := os.ReadFile(filepath.Join(state, "keys", keys[0].ID+".json"))
	if err != nil || strings.Contains(string(data), token) || strings.Contains(out, token) {
		t.Fatal("key must not appear in stored record or list")
	}
	_, diagnostics, err = cli(t, "keys", "revoke", "--state-dir", state, keys[0].ID)
	if err != nil {
		t.Fatalf("revoke: %v: %s", err, diagnostics)
	}
	out, _, err = cli(t, "keys", "list", "--state-dir", state)
	if err != nil || strings.TrimSpace(out) != "[]" {
		t.Fatalf("list after revoke: %q %v", out, err)
	}
}

func TestHelpAndInvalidArguments(t *testing.T) {
	out, _, err := cli(t, "--help")
	if err != nil || !strings.Contains(out, "Get started:") {
		t.Fatalf("help: %q %v", out, err)
	}
	_, diagnostics, err := cli(t, "keys", "revoke", "--help")
	if err != nil || !strings.Contains(diagnostics, "KEY_ID") {
		t.Fatalf("revoke help: %q %v", diagnostics, err)
	}
	_, diagnostics, err = cli(t, "serve", "--port", "0")
	if err == nil || !strings.Contains(diagnostics, "port must be") {
		t.Fatalf("invalid port: %q %v", diagnostics, err)
	}
	_, _, err = cli(t, "keys", "create", "--models", "bad scope")
	if err == nil {
		t.Fatal("invalid scope succeeded")
	}
}

func TestHelpDoesNotRecurseAndFlagsAreScoped(t *testing.T) {
	for _, args := range [][]string{
		{"help", "help"}, {"--help", "--help"}, {"keys", "list", "--name", "ignored"},
		{"keys", "revoke", "--models", "*", "id"}, {"serve", "--timeout", "0s"}, {"serve", "--max-concurrent", "0"},
	} {
		_, _, err := cli(t, args...)
		if err == nil {
			t.Errorf("expected an error for %v", args)
		}
	}
	_, diagnostics, err := cli(t, "help", "serve")
	if err != nil || !strings.Contains(diagnostics, "Usage: h2a serve") {
		t.Fatalf("help serve: %v %s", err, diagnostics)
	}
}

func TestBobRequiresExplicitPackage(t *testing.T) {
	for _, provider := range []string{"ibm-bob", "all"} {
		_, diagnostic, err := cli(t, "serve", "--provider", provider)
		if err == nil || !strings.Contains(diagnostic, "--pi-bob-extension") {
			t.Fatalf("provider %s: %s %v", provider, diagnostic, err)
		}
	}
	_, diagnostic, err := cli(t, "serve", "--provider", "ibm-bob", "--pi-bob-extension", t.TempDir())
	if err == nil || !strings.Contains(diagnostic, "extensions/bob.ts") {
		t.Fatalf("missing package: %s %v", diagnostic, err)
	}
}

func TestProviderSelection(t *testing.T) {
	for _, value := range []string{"", "codex,", "codex,codex", "all,codex", "unknown"} {
		if _, err := selectProviders(value); err == nil {
			t.Errorf("accepted %q", value)
		}
	}
	selected, err := selectProviders("codex, gemini,claude")
	if err != nil || strings.Join(selected, ",") != "codex,gemini,claude" {
		t.Fatalf("%v %v", selected, err)
	}
	selected, err = selectProviders("all")
	if err != nil || len(selected) != 6 {
		t.Fatalf("all: %v %v", selected, err)
	}
}
