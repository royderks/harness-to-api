package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"harness-to-api/internal/api"
	"harness-to-api/internal/auth"
)

func splitList(s string) []string {
	if s == "" {
		return nil
	}
	parts := strings.Split(s, ",")
	for i := range parts {
		parts[i] = strings.TrimSpace(parts[i])
	}
	return parts
}

func keyCommand(args []string) error {
	if len(args) == 0 || args[0] == "help" || args[0] == "--help" || args[0] == "-h" {
		fmt.Fprintln(os.Stderr, "Usage: h2a keys <create|list|revoke|inspect|explain> [flags]\n\nCreate prints a secret once; inspect and explain show read-only snapshots. Flags precede KEY_ID.")
		return nil
	}
	command := args[0]
	switch command {
	case "create", "list", "revoke", "inspect", "explain":
	default:
		return fmt.Errorf("unknown keys command %q", command)
	}
	f := flags("keys " + command)
	if command == "revoke" || command == "inspect" || command == "explain" {
		f.Usage = func() {
			fmt.Fprintf(f.Output(), "Usage: h2a keys %s [flags] KEY_ID\n\nFlags:\n", command)
			f.PrintDefaults()
		}
	}
	state := f.String("state-dir", stateDefault(), "directory for hashed API keys")
	var name, models, ttl, expires, timeout, maximum, rate, concurrent, provider, allowed, model string
	var validate bool
	if command == "create" {
		f.StringVar(&name, "name", "default", "key label")
		f.StringVar(&models, "models", "*", "comma-separated model scopes")
		f.StringVar(&ttl, "ttl", "", "key lifetime, e.g. 24h")
		f.StringVar(&expires, "expires-at", "", "expiry in RFC3339 format")
		f.StringVar(&timeout, "request-timeout", "", "per-request timeout, up to 1h")
		f.StringVar(&maximum, "max-requests", "", "lifetime request quota")
		f.StringVar(&rate, "rate-limit", "", "accepted requests per minute, e.g. 60/min")
		f.StringVar(&concurrent, "max-concurrent", "", "maximum concurrent requests")
		f.BoolVar(&validate, "validate", false, "validate exact scopes against the local catalog")
	}
	if command == "create" || command == "explain" {
		f.StringVar(&provider, "provider", "codex", "enabled providers, comma-separated, or all")
		f.StringVar(&allowed, "allowed-models", "", "comma-separated strict model catalog")
	}
	if command == "explain" {
		f.StringVar(&model, "model", "", "model to check")
	}
	if err := f.Parse(args[1:]); err != nil {
		return err
	}
	if command == "create" || command == "list" {
		if f.NArg() != 0 {
			return fmt.Errorf("keys %s takes flags only", command)
		}
	} else if f.NArg() != 1 {
		return fmt.Errorf("usage: h2a keys %s [flags] KEY_ID", command)
	}
	store := auth.NewStore(*state)
	switch command {
	case "create":
		policy, err := parsePolicy(f, ttl, expires, timeout, maximum, rate, concurrent)
		if err != nil {
			return err
		}
		scopes := splitList(models)
		if validate || flagSet(f, "provider") || flagSet(f, "allowed-models") {
			providers, err := selectProviders(provider)
			if err != nil {
				return err
			}
			catalog := splitList(allowed)
			if err = api.ValidateModels(catalog, providers); err != nil {
				return err
			}
			if validate {
				if len(catalog) == 0 {
					return errors.New("--validate requires --allowed-models")
				}
				for _, scope := range scopes {
					if err = api.ValidateModels([]string{scope}, providers); err != nil {
						return err
					}
					if err = api.CheckModel(scope, scopes, providers, catalog); err != nil {
						return err
					}
				}
			}
		}
		token, p, err := store.Create(name, scopes, policy)
		if err != nil {
			return err
		}
		fmt.Println(token)
		fmt.Fprintf(os.Stderr, "Created key %s. Save stdout securely; this key will not be shown again.\n", p.ID)
		return nil
	case "list":
		principals, err := store.List()
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(principals)
	case "revoke":
		if err := store.Revoke(f.Arg(0)); err != nil {
			return err
		}
		fmt.Fprintln(os.Stderr, "Key revoked; new requests will be rejected and active runs will be canceled when the server observes revocation.")
		return nil
	case "inspect":
		inspection, err := store.Inspect(f.Arg(0))
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(inspection)
	case "explain":
		providers, err := selectProviders(provider)
		if err != nil {
			return err
		}
		catalog := splitList(allowed)
		if err = api.ValidateModels(catalog, providers); err != nil {
			return err
		}
		inspection, err := store.Inspect(f.Arg(0))
		if err != nil {
			return err
		}
		result := struct {
			Inspection         auth.Inspection `json:"key"`
			Model              string          `json:"model,omitempty"`
			ModelAllowed       *bool           `json:"model_allowed,omitempty"`
			ModelReason        string          `json:"model_reason,omitempty"`
			AccountAccess      string          `json:"account_access"`
			Note               string          `json:"note"`
			LocalPolicyAllowed *bool           `json:"local_policy_allowed,omitempty"`
			Denials            []string        `json:"denials"`
			Providers          []string        `json:"configured_providers"`
			AllowedModels      []string        `json:"configured_models,omitempty"`
		}{Inspection: inspection, Model: model, AccountAccess: "unknown", Note: "Local snapshot using the supplied configuration, not the running server. No live concurrency or upstream account access is checked.", Providers: providers, AllowedModels: catalog, Denials: []string{}}
		if model != "" {
			e := api.CheckModel(model, inspection.Models, providers, catalog)
			ok := e == nil
			result.ModelAllowed = &ok
			if e != nil {
				result.ModelReason = e.Error()
				result.Denials = append(result.Denials, e.Error())
			}
			if inspection.Expired {
				result.Denials = append(result.Denials, "key expired")
			}
			if !inspection.Can("completions:create") {
				result.Denials = append(result.Denials, "completion permission missing")
			}
			if inspection.RemainingRequests != nil && *inspection.RemainingRequests == 0 {
				result.Denials = append(result.Denials, "lifetime request quota exhausted")
			}
			if inspection.Policy.RequestsPerMinute > 0 && inspection.RecentRequests >= inspection.Policy.RequestsPerMinute {
				result.Denials = append(result.Denials, "request rate limit reached")
			}
			localAllowed := len(result.Denials) == 0
			result.LocalPolicyAllowed = &localAllowed
		}
		return json.NewEncoder(os.Stdout).Encode(result)
	}
	return nil
}
func flagSet(f *flag.FlagSet, name string) bool {
	found := false
	f.Visit(func(v *flag.Flag) {
		if v.Name == name {
			found = true
		}
	})
	return found
}
func parsePolicy(f *flag.FlagSet, ttl, expires, timeout, maximum, rate, concurrent string) (auth.Policy, error) {
	var p auth.Policy
	if flagSet(f, "ttl") && flagSet(f, "expires-at") {
		return p, errors.New("--ttl and --expires-at are mutually exclusive")
	}
	if flagSet(f, "ttl") {
		d, e := time.ParseDuration(ttl)
		if e != nil || d <= 0 {
			return p, errors.New("--ttl must be a positive duration")
		}
		t := time.Now().Add(d)
		p.ExpiresAt = &t
	}
	if flagSet(f, "expires-at") {
		t, e := time.Parse(time.RFC3339, expires)
		if e != nil || !t.After(time.Now()) {
			return p, errors.New("--expires-at must be a future RFC3339 timestamp")
		}
		p.ExpiresAt = &t
	}
	if flagSet(f, "request-timeout") {
		d, e := time.ParseDuration(timeout)
		if e != nil || d <= 0 || d > time.Hour {
			return p, errors.New("--request-timeout must be positive and at most 1h")
		}
		p.RequestTimeout = auth.Duration(d)
	}
	if flagSet(f, "max-requests") {
		n, e := strconv.ParseUint(maximum, 10, 64)
		if e != nil || n == 0 {
			return p, errors.New("--max-requests must be a positive integer")
		}
		p.MaxRequests = n
	}
	if flagSet(f, "rate-limit") {
		if !strings.HasSuffix(rate, "/min") {
			return p, errors.New("--rate-limit must be N/min")
		}
		n, e := strconv.Atoi(strings.TrimSuffix(rate, "/min"))
		if e != nil || n < 1 || n > 100000 {
			return p, errors.New("--rate-limit must be 1/min through 100000/min")
		}
		p.RequestsPerMinute = n
	}
	if flagSet(f, "max-concurrent") {
		n, e := strconv.Atoi(concurrent)
		if e != nil || n < 1 || n > 128 {
			return p, errors.New("--max-concurrent must be 1–128")
		}
		p.MaxConcurrent = n
	}
	return p, nil
}
