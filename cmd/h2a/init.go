package main

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	"harness-to-api/internal/api"
	"harness-to-api/internal/auth"
	"harness-to-api/internal/initprofile"
)

type initFailure struct {
	status     int
	code, next string
}

func (e *initFailure) Error() string               { return e.code + ": " + e.next }
func initFail(status int, code, next string) error { return &initFailure{status, code, next} }

type initOutput struct{ json bool }

func (o initOutput) emit(event string, fields map[string]any) error {
	fields["schema_version"], fields["event"] = 1, event
	if o.json {
		return json.NewEncoder(os.Stdout).Encode(fields)
	}
	switch event {
	case "plan":
		fmt.Fprintf(os.Stdout, "Plan: %s; %s; model %s\nState: %s\nClient credential: %s\nKey: 1h from creation, 20 accepted requests, 90s/request, 5/minute, concurrency 1.\nForeground until Ctrl+C. Limits are not provider dollar limits.\nBinary found; login unknown; model access unknown.\n", fields["provider"], fields["endpoint"], fields["model"], fields["state_dir"], fields["credential_path"])
	case "ready":
		fmt.Fprintf(os.Stdout, "Ready (unverified): %s; model %s\nKey ID: %s; expires %s; remaining %v\nRead the protected credential file in your client: %s\nPython client example:\n%s\nStop: Ctrl+C. Revoke: h2a keys revoke --state-dir %s %s\n", fields["endpoint"], fields["model"], fields["key_id"], fields["expires_at"], fields["remaining_requests"], fields["credential_path"], fields["client_example"], strconv.Quote(fmt.Sprint(fields["state_dir"])), fields["key_id"])
	case "probe":
		fmt.Fprintf(os.Stdout, "Probe: %s (one quota-counted request; account access is evidence for this request only).\n", fields["verification"])
	case "stopped":
		fmt.Fprintf(os.Stdout, "Stopped: %s.\n", fields["reason"])
	case "error":
		fmt.Fprintf(os.Stderr, "h2a init: %s; %s\n", fields["code"], fields["next_step"])
	}
	return nil
}

func initCommand(args []string) error {
	output := initOutput{}
	for _, arg := range args {
		if arg == "--json" || arg == "--json=true" {
			output.json = true
		}
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	var err error
	if len(args) > 0 && args[0] == "repair" {
		err = initRepair(ctx, args[1:], &output)
	} else {
		err = initStart(ctx, args, &output)
	}
	if err == nil || errors.Is(err, flag.ErrHelp) {
		return err
	}
	var failure *initFailure
	if !errors.As(err, &failure) {
		failure = &initFailure{1, "io_error", "Check private local storage and retry; interrupted setup may require h2a init repair."}
	}
	_ = output.emit("error", map[string]any{"code": failure.code, "next_step": failure.next})
	return failure
}

func initPrompt(ctx context.Context, reader *bufio.Reader, label, fallback string) (string, error) {
	fmt.Fprintf(os.Stderr, "%s [%s]: ", label, fallback)
	type answer struct {
		text string
		err  error
	}
	ch := make(chan answer, 1)
	go func() { line, err := reader.ReadString('\n'); ch <- answer{strings.TrimSpace(line), err} }()
	select {
	case <-ctx.Done():
		return "", initFail(130, "cancelled", "Setup cancelled before readiness.")
	case a := <-ch:
		if a.err != nil {
			return "", initFail(2, "input_required", "Use explicit --provider, --model, --state-dir, --port and --probe or --no-probe flags.")
		}
		if a.text == "" {
			return fallback, nil
		}
		return a.text, nil
	}
}
func terminalInput() bool {
	info, err := os.Stdin.Stat()
	return err == nil && info.Mode()&os.ModeCharDevice != 0
}
func initParse(f *flag.FlagSet, args []string) error {
	f.SetOutput(io.Discard)
	err := parse(f, args)
	if errors.Is(err, flag.ErrHelp) {
		f.SetOutput(os.Stdout)
		f.Usage()
		return err
	}
	if err != nil {
		return initFail(2, "invalid_arguments", "Run h2a init --help (or h2a init repair --help) for supported flags.")
	}
	return nil
}

func initStart(ctx context.Context, args []string, output *initOutput) error {
	f := flags("init")
	provider := f.String("provider", "", "one installed harness: codex, antigravity, ibm-bob, gemini, claude, opencode")
	model := f.String("model", "", "exact provider/model; default aliases do not pin upstream versions")
	state := f.String("state-dir", stateDefault(), "private local state directory")
	port := f.Int("port", 8787, "loopback port")
	probe := f.Bool("probe", false, "send one real authenticated completion; consumes quota")
	noProbe := f.Bool("no-probe", false, "start without inference; access remains unverified")
	bob := f.String("pi-bob-extension", "", "absolute installed community pi-bob package path")
	f.BoolVar(&output.json, "json", output.json, "secret-free NDJSON; no prompts")
	if err := initParse(f, args); err != nil {
		return err
	}
	seen := map[string]bool{}
	f.Visit(func(v *flag.Flag) { seen[v.Name] = true })
	interactive := terminalInput() && !output.json
	if seen["probe"] && seen["no-probe"] || seen["probe"] && !*probe || seen["no-probe"] && !*noProbe {
		return initFail(2, "probe_choice", "Choose exactly one of --probe or --no-probe.")
	}
	if !interactive {
		for _, name := range []string{"provider", "model", "state-dir", "port"} {
			if !seen[name] {
				return initFail(2, "input_required", "Supply --provider, --model, --state-dir, --port and --probe or --no-probe.")
			}
		}
		if !*probe && !*noProbe {
			return initFail(2, "probe_choice", "Choose --no-probe for offline setup or --probe to consume one request.")
		}
	}
	reader := bufio.NewReader(os.Stdin)
	if interactive {
		ids := []string{"codex", "antigravity", "ibm-bob", "gemini", "claude", "opencode"}
		binaries := []string{"codex", "agy", "pi", "gemini", "claude", "opencode"}
		first := ""
		for i, id := range ids {
			status := "missing"
			if _, err := exec.LookPath(binaries[i]); err == nil {
				status = "found"
				if first == "" {
					first = id
				}
			}
			fmt.Fprintf(os.Stderr, "%s: binary %s; login unknown; model access unknown\n", id, status)
		}
		var err error
		if *provider == "" {
			if first == "" {
				return initFail(3, "harness_missing", "Install and sign in to a supported official harness, then rerun init.")
			}
			*provider, err = initPrompt(ctx, reader, "Provider", first)
			if err != nil {
				return err
			}
		}
		if *model == "" {
			*model, err = initPrompt(ctx, reader, "Model", *provider+"/default")
			if err != nil {
				return err
			}
		}
		if !seen["port"] {
			value, err := initPrompt(ctx, reader, "Port", "8787")
			if err != nil {
				return err
			}
			*port, err = strconv.Atoi(value)
			if err != nil {
				return initFail(2, "invalid_port", "Use a port from 1 to 65535.")
			}
		}
		if !seen["state-dir"] {
			*state, err = initPrompt(ctx, reader, "State directory", *state)
			if err != nil {
				return err
			}
		}
		if *provider == "ibm-bob" && *bob == "" {
			*bob, err = initPrompt(ctx, reader, "Community pi-bob package (absolute path)", "")
			if err != nil {
				return err
			}
		}
	}
	selected, err := selectProviders(*provider)
	if err != nil || len(selected) != 1 {
		return initFail(2, "invalid_provider", "Choose exactly one supported provider; see h2a init --help.")
	}
	if api.CheckModel(*model, []string{*model}, selected, nil) != nil {
		return initFail(2, "invalid_model", "Use an exact provider/model for the selected provider; wildcards are not allowed.")
	}
	if *port < 1 || *port > 65535 {
		return initFail(2, "invalid_port", "Use a port from 1 to 65535.")
	}
	if strings.TrimSpace(*state) == "" {
		return initFail(2, "invalid_state", "Choose a private local state directory.")
	}
	*state, err = filepath.Abs(*state)
	if err != nil {
		return initFail(2, "invalid_state", "Choose a valid private local state directory.")
	}
	bins := providerBinaries{"codex", "agy", "pi", "gemini", "claude", "opencode", *bob}
	adapters, err := buildAdapters(selected, bins)
	if err != nil {
		return initFail(3, "harness_missing", "Install the selected harness and sign in using its official CLI; community IBM Bob also needs --pi-bob-extension containing extensions/bob.ts.")
	}
	endpoint := "http://127.0.0.1:" + strconv.Itoa(*port) + "/v1"
	plan := map[string]any{"provider": *provider, "endpoint": endpoint, "model": *model, "state_dir": *state, "credential_path": filepath.Join(*state, "init", "client.key"), "lifetime": "foreground", "key_ttl": "1h", "max_requests": 20, "request_timeout": "90s", "requests_per_minute": 5, "max_concurrent": 1, "binary": "found", "login": "unknown", "model_access": "unknown", "probe_requested": *probe}
	if err := output.emit("plan", plan); err != nil {
		return err
	}
	if interactive {
		yes, err := initPrompt(ctx, reader, "Start or resume this setup? y/N", "n")
		if err != nil {
			return err
		}
		if !strings.EqualFold(yes, "y") && !strings.EqualFold(yes, "yes") {
			return output.emit("stopped", map[string]any{"reason": "declined"})
		}
		if !seen["probe"] && !seen["no-probe"] {
			yes, err = initPrompt(ctx, reader, "Run a real probe (consumes quota)? y/N", "n")
			if err != nil {
				return err
			}
			*probe = strings.EqualFold(yes, "y") || strings.EqualFold(yes, "yes")
		}
	}
	if ctx.Err() != nil {
		return initFail(130, "cancelled", "Setup cancelled before readiness.")
	}
	files, err := initprofile.Open(*state)
	if err != nil {
		return initFail(3, "unsafe_storage", "Use a private local state directory with owner-only access, regular files and no symbolic links; do not reuse an unsafe destination.")
	}
	defer files.Close()
	keys := auth.NewStore(*state)
	app, err := api.New(api.Config{Keys: keys, Adapters: adapters, Timeout: 90 * time.Second, MaxConcurrent: 1})
	if err != nil {
		return initFail(3, "state_unavailable", "Stop the gateway already using this state directory, or check its private local storage; do not remove lock files.")
	}
	defer app.Close()
	listener, err := net.Listen("tcp", net.JoinHostPort("127.0.0.1", strconv.Itoa(*port)))
	if err != nil {
		return initFail(3, "port_in_use", "The loopback port is unavailable. Stop your own listener or select a free port for a fresh setup; no existing process was contacted.")
	}
	defer listener.Close()
	profile, token, err := files.Load()
	if err != nil {
		return initFail(3, "repair_required", "Onboarding files are corrupt or unsafe. Inspect the private profile and key ID; repair only identified onboarding artifacts, then rerun init.")
	}
	if profile != nil && (profile.Provider != *provider || profile.Model != *model || profile.Port != *port || profile.BobExtension != *bob) {
		return initFail(3, "configuration_mismatch", "Reuse the original provider, model, port and extension, or stop and run h2a init repair --state-dir PATH --discard --yes.")
	}
	if profile == nil {
		var principal auth.Principal
		token, principal, err = auth.Prepare("onboarding", []string{*model}, auth.Policy{RequestTimeout: auth.Duration(90 * time.Second), MaxRequests: 20, RequestsPerMinute: 5, MaxConcurrent: 1})
		if err != nil {
			return err
		}
		expires := principal.CreatedAt.Add(time.Hour)
		principal.Policy.ExpiresAt = &expires
		profile = &initprofile.Profile{Version: 1, Phase: initprofile.Pending, Provider: *provider, Model: *model, Port: *port, BobExtension: *bob, Principal: principal}
		if err = files.SaveProfile(*profile); err != nil {
			return err
		}
		if err = files.SaveCredential(token); err != nil {
			return err
		}
		if err = keys.InsertPrepared(token, principal); err != nil {
			return err
		}
	}
	inspection, err := initprofile.Resume(keys, *profile, token)
	if err != nil {
		code := "repair_required"
		switch {
		case errors.Is(err, initprofile.ErrExpired):
			code = "key_expired"
		case errors.Is(err, initprofile.ErrExhausted):
			code = "key_exhausted"
		case errors.Is(err, initprofile.ErrMissing):
			code = "key_missing"
		}
		return initFail(3, code, "Stop and run h2a init repair --state-dir PATH --discard --yes; a repeat never replaces missing or revoked credentials or resets key limits.")
	}
	if profile.Phase != initprofile.Ready {
		profile.Phase = initprofile.Ready
		if err := files.SaveProfile(*profile); err != nil {
			return err
		}
	}
	if ctx.Err() != nil {
		return initFail(130, "cancelled", "Setup cancelled before readiness; rerun init to resume the same key.")
	}
	lifetime, cancel := context.WithCancel(ctx)
	defer cancel()
	server := gatewayHTTPServer(lifetime, app)
	served := make(chan error, 1)
	go func() { served <- server.Serve(listener) }()
	defer func() {
		cancel()
		drain, done := context.WithTimeout(context.Background(), 5*time.Second)
		defer done()
		_ = server.Shutdown(drain)
		_ = server.Close()
	}()
	ready := map[string]any{"provider": *provider, "endpoint": endpoint, "model": *model, "state_dir": *state, "key_id": profile.Principal.ID, "credential_path": files.CredentialPath(), "expires_at": profile.Principal.Policy.ExpiresAt.Format(time.RFC3339), "remaining_requests": *inspection.RemainingRequests, "verification": "unverified", "client_example": initClientExample(files.CredentialPath(), endpoint, *model)}
	if err := output.emit("ready", ready); err != nil {
		return err
	}
	if *probe {
		result, err := executeProbe(lifetime, endpoint, *model, token)
		if ctx.Err() != nil {
			return output.emit("stopped", map[string]any{"reason": "signal"})
		}
		verification := "unverified"
		if err == nil && result.Available {
			verification = "verified"
		}
		if err := output.emit("probe", map[string]any{"verification": verification, "checked_at": result.CheckedAt, "http_status": result.HTTPStatus, "model": *model}); err != nil {
			return err
		}
		if verification != "verified" {
			return initFail(4, "probe_failed", "Check the official harness login, model access and key limits; rerun with --no-probe to start unverified. An admitted probe consumes quota even on failure.")
		}
	}
	select {
	case <-ctx.Done():
		return output.emit("stopped", map[string]any{"reason": "signal"})
	case err := <-served:
		if errors.Is(err, http.ErrServerClosed) {
			return output.emit("stopped", map[string]any{"reason": "closed"})
		}
		return initFail(1, "server_failed", "The listener stopped unexpectedly; check local resources and rerun init to resume.")
	}
}

func initClientExample(path, endpoint, model string) string {
	// JSON string literals are also valid Python string literals for these values.
	quoted := func(s string) string { value, _ := json.Marshal(s); return string(value) }
	return "import json, pathlib, urllib.request\nkey = pathlib.Path(" + quoted(path) + ").read_text().strip()\nbody = json.dumps({'model': " + quoted(model) + ", 'messages': [{'role': 'user', 'content': 'Say hello.'}]}).encode()\nrequest = urllib.request.Request(" + quoted(endpoint+"/chat/completions") + ", data=body, headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'})\nwith urllib.request.urlopen(request) as response:\n    print(response.read().decode())"
}

func initRepair(ctx context.Context, args []string, output *initOutput) error {
	f := flags("init repair")
	state := f.String("state-dir", stateDefault(), "private local state directory")
	discard := f.Bool("discard", false, "revoke the recorded onboarding key and discard its client profile")
	yes := f.Bool("yes", false, "confirm discard without prompting")
	f.BoolVar(&output.json, "json", output.json, "secret-free NDJSON; no prompts")
	if err := initParse(f, args); err != nil {
		return err
	}
	if !*discard {
		return initFail(2, "discard_required", "Use --discard to explicitly revoke and remove this onboarding setup.")
	}
	if !*yes {
		if output.json || !terminalInput() {
			return initFail(2, "confirmation_required", "Use --discard --yes to explicitly confirm repair without prompts.")
		}
		value, err := initPrompt(ctx, bufio.NewReader(os.Stdin), "Revoke and discard onboarding? y/N", "n")
		if err != nil {
			return err
		}
		if !strings.EqualFold(value, "y") && !strings.EqualFold(value, "yes") {
			return output.emit("stopped", map[string]any{"reason": "declined"})
		}
	}
	if strings.TrimSpace(*state) == "" {
		return initFail(2, "invalid_state", "Choose the original private state directory.")
	}
	if ctx.Err() != nil {
		return initFail(130, "cancelled", "Repair cancelled before changes.")
	}
	files, err := initprofile.Open(*state)
	if err != nil {
		return initFail(3, "unsafe_storage", "Repair requires safe private local storage; inspect permissions and symbolic links without deleting unrelated files.")
	}
	defer files.Close()
	keys := auth.NewStore(*state)
	owner, err := keys.OpenAdmission(90*time.Second, 1)
	if err != nil {
		return initFail(3, "state_unavailable", "Stop the gateway using this state before repair; do not remove lock files.")
	}
	defer owner.Close()
	profile, _, err := files.Load()
	if err != nil {
		return initFail(3, "repair_required", "Cannot establish the onboarding identity. Inspect the private profile and revoke only its verified key ID; do not guess or delete other keys.")
	}
	if profile != nil {
		profile.Phase = initprofile.Discarding
		if err := files.SaveProfile(*profile); err != nil {
			return err
		}
		if err := keys.Revoke(profile.Principal.ID); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		if err := files.RemoveCredential(); err != nil {
			return err
		}
		if err := files.RemoveProfile(); err != nil {
			return err
		}
	}
	return output.emit("stopped", map[string]any{"reason": "onboarding discarded; run init for a new bounded key"})
}

func gatewayHTTPServer(ctx context.Context, handler http.Handler) *http.Server {
	return &http.Server{Handler: handler, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 15 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 * 1024, BaseContext: func(net.Listener) context.Context { return ctx }}
}
