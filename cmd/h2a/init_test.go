package main

import (
	"encoding/json"
	"errors"
	"os/exec"
	"strings"
	"testing"
)

func TestInitHelpAndMachineErrors(t *testing.T) {
	for _, args := range [][]string{{"init", "--help"}, {"help", "init"}, {"init", "repair", "--help"}} {
		out, _, err := cli(t, args...)
		if err != nil || !strings.Contains(out, "Usage: h2a init") {
			t.Fatalf("help: %v %q", err, out)
		}
	}
	tests := []struct {
		args []string
		code string
	}{
		{[]string{"init", "--json"}, "input_required"},
		{[]string{"init", "--bad", "-json"}, "invalid_arguments"},
		{[]string{"init", "repair", "--bad", "-json=true"}, "invalid_arguments"},
		{[]string{"init", "--json=false", "--bad", "-json=1"}, "invalid_arguments"},
		{[]string{"init", "--provider", "codex", "--model", "codex/default", "--state-dir", "unused", "--port", "8787", "--no-probe", "--pi-bob-extension", "/tmp", "--json"}, "invalid_extension"},
		{[]string{"init", "--unknown", "private-input", "--json"}, "invalid_arguments"},
		{[]string{"init", "--provider", "codex", "--model", "codex/default", "--state-dir", "unused", "--port", "8787", "--probe", "--no-probe", "--json"}, "probe_choice"},
		{[]string{"init", "--provider", "codex", "--model", "gemini/default", "--state-dir", "unused", "--port", "8787", "--no-probe", "--json"}, "invalid_model"},
		{[]string{"init", "--provider", "codex", "--model", "codex/*", "--state-dir", "unused", "--port", "8787", "--no-probe", "--json"}, "invalid_model"},
		{[]string{"init", "--provider", "all", "--model", "codex/default", "--state-dir", "unused", "--port", "8787", "--no-probe", "--json"}, "invalid_provider"},
		{[]string{"init", "--provider", "codex", "--model", "codex/default", "--state-dir", "unused", "--port", "0", "--no-probe", "--json"}, "invalid_port"},
		{[]string{"init", "repair", "--json"}, "discard_required"},
		{[]string{"init", "repair", "--discard", "--json"}, "confirmation_required"},
	}
	for _, tc := range tests {
		t.Run(tc.code, func(t *testing.T) {
			out, diagnostic, err := cli(t, tc.args...)
			var exit *exec.ExitError
			if !errors.As(err, &exit) || exit.ExitCode() != 2 {
				t.Fatalf("expected exit2; got %v", err)
			}
			var event struct {
				Version               int `json:"schema_version"`
				Event, Code, NextStep string
			}
			if json.Unmarshal([]byte(out), &event) != nil || event.Version != 1 || event.Event != "error" || event.Code != tc.code {
				t.Fatalf("invalid error: %q", out)
			}
			if strings.Contains(out+diagnostic, "private-input") || strings.TrimSpace(diagnostic) != "" {
				t.Fatal("machine errors must not reflect raw arguments or duplicate diagnostics")
			}
		})
	}
}
