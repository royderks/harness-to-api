package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"harness-to-api/internal/api"
	"harness-to-api/internal/auth"
	"harness-to-api/internal/harness"
)

var version = "dev"

const help = `h2a — Your coding harness, as an authenticated OpenAI-compatible API.

Usage:
  h2a doctor
  h2a keys create [--name local] [--models 'codex/*'] [--state-dir PATH]
  h2a keys list|inspect|explain [flags]
  h2a probe --model provider/model [--base-url URL]
  h2a keys revoke [--state-dir PATH] KEY_ID
  h2a serve [--provider codex[,gemini,...]|all] [--port 8787]
  h2a share [--via tailscale|funnel|cloudflare] [--port 8787]
  h2a version

Get started:
  1. Install and sign in to an official harness CLI (for Codex: codex login).
  2. export OPENAI_API_KEY="$(h2a keys create --name local --models 'codex/*')"
  3. h2a serve --provider codex
  4. Point your OpenAI client at http://127.0.0.1:8787/v1.

Use h2a <command> --help for flags. A new API key is printed once to stdout;
save it securely. Set HARNESS_API_STATE_DIR to choose where key hashes are stored.
`

func stateDefault() string {
	if value := os.Getenv("HARNESS_API_STATE_DIR"); value != "" {
		return value
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return ".harness-to-api"
	}
	return filepath.Join(home, ".harness-to-api")
}
func flags(name string) *flag.FlagSet {
	f := flag.NewFlagSet(name, flag.ContinueOnError)
	f.Usage = func() {
		fmt.Fprintf(f.Output(), "Usage: h2a %s [flags]\n\nFlags:\n", name)
		f.PrintDefaults()
	}
	return f
}
func parse(f *flag.FlagSet, args []string) error {
	if err := f.Parse(args); err != nil {
		return err
	}
	if f.NArg() != 0 {
		return fmt.Errorf("unexpected argument %q", f.Arg(0))
	}
	return nil
}
func main() {
	if err := run(os.Args[1:]); err != nil && !errors.Is(err, flag.ErrHelp) {
		fmt.Fprintln(os.Stderr, "h2a:", err)
		os.Exit(1)
	}
}
func run(args []string) error {
	if len(args) == 0 {
		fmt.Print(help)
		return nil
	}
	switch args[0] {
	case "help":
		if len(args) > 1 {
			switch args[1] {
			case "serve", "keys", "doctor", "share", "probe":
				return run(append(append([]string{}, args[1:]...), "--help"))
			default:
				return fmt.Errorf("unknown help topic %q; run h2a --help", args[1])
			}
		}
		fmt.Print(help)
		return nil
	case "--help", "-h":
		if len(args) != 1 {
			return errors.New("usage: h2a --help or h2a help COMMAND")
		}
		fmt.Print(help)
		return nil
	case "version", "--version":
		if len(args) != 1 {
			return errors.New("usage: h2a version")
		}
		fmt.Println(version)
		return nil
	case "keys":
		return keyCommand(args[1:])
	case "probe":
		return probeCommand(args[1:])
	case "serve":
		return serveCommand(args[1:])
	case "doctor":
		return doctor(args[1:])
	case "share":
		return share(args[1:])
	default:
		return fmt.Errorf("unknown command %q; run h2a --help", args[0])
	}
}
func selectProviders(value string) ([]string, error) {
	all := []string{"codex", "antigravity", "ibm-bob", "gemini", "claude", "opencode"}
	if value == "all" {
		return all, nil
	}
	selected := strings.Split(value, ",")
	seen := map[string]bool{}
	for i, name := range selected {
		name = strings.TrimSpace(name)
		known := false
		for _, candidate := range all {
			if name == candidate {
				known = true
				break
			}
		}
		if !known || seen[name] {
			return nil, fmt.Errorf("invalid or repeated provider %q; choose from %s, or all", name, strings.Join(all, ", "))
		}
		selected[i], seen[name] = name, true
	}
	return selected, nil
}

type providerBinaries struct{ codex, antigravity, pi, gemini, claude, opencode, bobExtension string }

func buildAdapters(selected []string, bins providerBinaries) ([]harness.Adapter, error) {
	configured := map[string]struct {
		adapter harness.Adapter
		binary  string
	}{
		"codex":       {harness.Codex{Binary: bins.codex}, bins.codex},
		"antigravity": {harness.Antigravity{Binary: bins.antigravity}, bins.antigravity},
		"ibm-bob":     {harness.IBMBob{Binary: bins.pi, Extension: bins.bobExtension}, bins.pi},
		"gemini":      {harness.Gemini{Binary: bins.gemini}, bins.gemini},
		"claude":      {harness.Claude{Binary: bins.claude}, bins.claude},
		"opencode":    {harness.OpenCode{Binary: bins.opencode}, bins.opencode},
	}
	for _, id := range selected {
		if id == "ibm-bob" {
			if !filepath.IsAbs(bins.bobExtension) {
				return nil, errors.New("--pi-bob-extension must be an absolute path to the installed pi-bob package")
			}
			if info, err := os.Stat(filepath.Join(bins.bobExtension, "extensions", "bob.ts")); err != nil || !info.Mode().IsRegular() {
				return nil, errors.New("--pi-bob-extension must contain extensions/bob.ts; install pi-bob first")
			}
		}
	}
	adapters := make([]harness.Adapter, 0, len(selected))
	for _, id := range selected {
		entry := configured[id]
		if _, err := exec.LookPath(entry.binary); err != nil {
			return nil, fmt.Errorf("%s CLI not found (%s); install and sign in first", id, entry.binary)
		}
		adapters = append(adapters, entry.adapter)
	}
	return adapters, nil
}

func serveCommand(args []string) error {
	f := flags("serve")
	state := f.String("state-dir", stateDefault(), "directory for hashed API keys")
	provider := f.String("provider", "codex", "comma-separated codex, antigravity, ibm-bob, gemini, claude, opencode; or all")
	allowed := f.String("allowed-models", "", "comma-separated strict model catalog")
	host := f.String("host", "127.0.0.1", "bind address")
	port := f.Int("port", 8787, "HTTP port")
	timeout := f.Duration("timeout", 2*time.Minute, "maximum request duration, e.g. 120s")
	concurrent := f.Int("max-concurrent", 4, "maximum active harness requests")
	codexBin := f.String("codex-bin", "codex", "official Codex executable")
	agyBin := f.String("antigravity-bin", "agy", "official Antigravity executable")
	piBin := f.String("pi-bin", "pi", "Pi executable")
	geminiBin := f.String("gemini-bin", "gemini", "Gemini CLI executable")
	claudeBin := f.String("claude-bin", "claude", "Claude Code executable")
	opencodeBin := f.String("opencode-bin", "opencode", "OpenCode executable")
	bobExtension := f.String("pi-bob-extension", "", "absolute path to installed pi-bob package")
	if err := parse(f, args); err != nil {
		return err
	}
	if *port < 1 || *port > 65535 {
		return errors.New("port must be 1–65535")
	}
	if *timeout <= 0 || *timeout > time.Hour {
		return errors.New("timeout must be greater than zero and no more than 1h")
	}
	if *concurrent < 1 || *concurrent > 128 {
		return errors.New("max-concurrent must be 1–128")
	}
	selected, err := selectProviders(*provider)
	if err != nil {
		return err
	}
	catalog := splitList(*allowed)
	if err := api.ValidateModels(catalog, selected); err != nil {
		return err
	}
	adapters, err := buildAdapters(selected, providerBinaries{*codexBin, *agyBin, *piBin, *geminiBin, *claudeBin, *opencodeBin, *bobExtension})
	if err != nil {
		return err
	}
	store := auth.NewStore(*state)
	list, err := store.List()
	if err != nil {
		return err
	}
	if len(list) == 0 {
		return errors.New("create a key first: h2a keys create --name local (use the same --state-dir)")
	}
	logger := slog.New(slog.NewJSONHandler(os.Stderr, nil))
	app, err := api.New(api.Config{Keys: store, Adapters: adapters, Timeout: *timeout, MaxConcurrent: *concurrent, Logger: logger, AllowedModels: catalog})
	if err != nil {
		return err
	}
	defer app.Close()
	lifetime, cancel := context.WithCancel(context.Background())
	defer cancel()
	server := &http.Server{Addr: net.JoinHostPort(*host, fmt.Sprint(*port)), Handler: app,
		ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 15 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 * 1024,
		BaseContext: func(net.Listener) context.Context { return lifetime },
	}
	listener, err := net.Listen("tcp", server.Addr)
	if err != nil {
		return fmt.Errorf("listen on %s: %w", server.Addr, err)
	}
	fmt.Fprintf(os.Stderr, "Serving %s at http://%s/v1 (press Ctrl+C to stop)\n", *provider, listener.Addr())
	logger.Info("listening", "address", listener.Addr().String(), "base_path", "/v1", "provider", *provider)
	signals, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	finished := make(chan struct{})
	go func() {
		select {
		case <-signals.Done():
			cancel()
			shutdown, done := context.WithTimeout(context.Background(), 5*time.Second)
			defer done()
			_ = server.Shutdown(shutdown)
			_ = server.Close()
		case <-finished:
		}
	}()
	err = server.Serve(listener)
	close(finished)
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}
func doctor(args []string) error {
	f := flags("doctor")
	piBin := f.String("pi-bin", "pi", "Pi executable")
	geminiBin := f.String("gemini-bin", "gemini", "Gemini CLI executable")
	claudeBin := f.String("claude-bin", "claude", "Claude Code executable")
	opencodeBin := f.String("opencode-bin", "opencode", "OpenCode executable")
	if err := parse(f, args); err != nil {
		return err
	}
	for _, binary := range []string{"codex", "agy", *piBin, *geminiBin, *claudeBin, *opencodeBin, "tailscale", "cloudflared"} {
		path, err := exec.LookPath(binary)
		if err != nil {
			fmt.Printf("%s: not installed\n", binary)
			continue
		}
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		cmd := exec.CommandContext(ctx, path, "--version")
		cmd.Stderr = io.Discard
		output, err := cmd.Output()
		cancel()
		if err != nil {
			fmt.Printf("%s: found at %s (version check failed)\n", binary, path)
		} else {
			fmt.Printf("%s: %s\n", binary, strings.TrimSpace(string(output)))
		}
	}
	fmt.Println("Login is not checked. Sign in with each CLI before serving it. For community pi-bob, use Pi /login ibm-bob. Tunnels are optional.")
	return nil
}
func share(args []string) error {
	f := flags("share")
	via := f.String("via", "tailscale", "tailscale (private), funnel (public), or cloudflare (named tunnel)")
	port := f.Int("port", 8787, "local API port")
	if err := parse(f, args); err != nil {
		return err
	}
	if *port < 1 || *port > 65535 {
		return errors.New("port must be 1–65535")
	}
	switch *via {
	case "tailscale", "funnel":
		command := "serve"
		visibility := "private tailnet"
		if *via == "funnel" {
			command = "funnel"
			visibility = "public internet"
		}
		fmt.Printf("tailscale %s http://127.0.0.1:%d\n", command, *port)
		fmt.Fprintf(os.Stderr, "Visibility: %s. Run the command above, then use its HTTPS URL plus /v1. A Bearer API key is still required.\n", visibility)
	case "cloudflare":
		fmt.Printf("Configure a named Cloudflare Tunnel HTTPS hostname pointing to http://127.0.0.1:%d.\nQuick Tunnels do not support SSE. See docs/remote-access.md.\n", *port)
	default:
		return errors.New("via must be tailscale, funnel, or cloudflare")
	}
	return nil
}
