package main

import (
	"context"
	"encoding/json"
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
  h2a keys create [--name local] [--models 'codex/*'] [--state-dir PATH]
  h2a keys list [--state-dir PATH]
  h2a keys revoke [--state-dir PATH] KEY_ID
  h2a serve [--provider codex|antigravity|all] [--port 8787]
  h2a doctor
  h2a share [--via tailscale|funnel|cloudflare] [--port 8787]
  h2a version

Create a key, sign in to the official harness CLI, then run h2a serve.
Use h2a <command> --help for flags. API keys are printed once to stdout.
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
func flags(name string) *flag.FlagSet { return flag.NewFlagSet(name, flag.ContinueOnError) }
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
	case "help", "--help", "-h":
		fmt.Print(help)
		return nil
	case "version", "--version":
		fmt.Println(version)
		return nil
	case "keys":
		return keyCommand(args[1:])
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
func keyCommand(args []string) error {
	if len(args) == 0 {
		return errors.New("use keys create, list, or revoke")
	}
	f := flags("keys " + args[0])
	state := f.String("state-dir", stateDefault(), "directory for hashed API keys")
	name := f.String("name", "default", "key label (create)")
	models := f.String("models", "*", "comma-separated model scopes (create)")
	if err := f.Parse(args[1:]); err != nil {
		return err
	}
	store := auth.NewStore(*state)
	switch args[0] {
	case "create":
		if f.NArg() != 0 {
			return errors.New("keys create takes flags only")
		}
		token, p, err := store.Create(*name, strings.Split(*models, ","))
		if err != nil {
			return err
		}
		fmt.Println(token)
		fmt.Fprintf(os.Stderr, "Created key %s. Save stdout securely; this key will not be shown again.\n", p.ID)
		return nil
	case "list":
		if f.NArg() != 0 {
			return errors.New("keys list takes flags only")
		}
		principals, err := store.List()
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(principals)
	case "revoke":
		if f.NArg() != 1 {
			return errors.New("usage: h2a keys revoke [--state-dir PATH] KEY_ID")
		}
		if err := store.Revoke(f.Arg(0)); err != nil {
			return err
		}
		fmt.Fprintln(os.Stderr, "Key revoked; new requests will be rejected.")
		return nil
	default:
		return errors.New("use keys create, list, or revoke")
	}
}
func serveCommand(args []string) error {
	f := flags("serve")
	state := f.String("state-dir", stateDefault(), "directory for hashed API keys")
	provider := f.String("provider", "codex", "codex, antigravity, or all")
	host := f.String("host", "127.0.0.1", "bind address")
	port := f.Int("port", 8787, "HTTP port")
	timeout := f.Duration("timeout", 2*time.Minute, "maximum request duration, e.g. 120s")
	concurrent := f.Int("max-concurrent", 4, "maximum active harness requests")
	codexBin := f.String("codex-bin", "codex", "official Codex executable")
	agyBin := f.String("antigravity-bin", "agy", "official Antigravity executable")
	if err := parse(f, args); err != nil {
		return err
	}
	if *port < 1 || *port > 65535 {
		return errors.New("port must be 1–65535")
	}
	adapters := []harness.Adapter{}
	switch *provider {
	case "codex":
		adapters = append(adapters, harness.Codex{Binary: *codexBin})
	case "antigravity":
		adapters = append(adapters, harness.Antigravity{Binary: *agyBin})
	case "all":
		adapters = append(adapters, harness.Codex{Binary: *codexBin}, harness.Antigravity{Binary: *agyBin})
	default:
		return errors.New("provider must be codex, antigravity, or all")
	}
	for _, adapter := range adapters {
		binary := *codexBin
		if adapter.ID() == "antigravity" {
			binary = *agyBin
		}
		if _, err := exec.LookPath(binary); err != nil {
			return fmt.Errorf("%s CLI not found (%s); install and sign in first", adapter.ID(), binary)
		}
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
	app, err := api.New(api.Config{Keys: store, Adapters: adapters, Timeout: *timeout, MaxConcurrent: *concurrent, Logger: logger})
	if err != nil {
		return err
	}
	lifetime, cancel := context.WithCancel(context.Background())
	defer cancel()
	server := &http.Server{Addr: net.JoinHostPort(*host, fmt.Sprint(*port)), Handler: app,
		ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 15 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 * 1024,
		BaseContext: func(net.Listener) context.Context { return lifetime },
	}
	listener, err := net.Listen("tcp", server.Addr)
	if err != nil {
		return err
	}
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
	if err := parse(f, args); err != nil {
		return err
	}
	for _, binary := range []string{"codex", "agy", "tailscale", "cloudflared"} {
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
	fmt.Println("Login is not checked. Use codex login or an interactive agy session. Tunnels are optional.")
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
