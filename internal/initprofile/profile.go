// Package initprofile defines the nonsecret onboarding journal and recovery checks.
package initprofile

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"time"

	"harness-to-api/internal/auth"
)

type Phase string

const (
	Pending    Phase = "pending"
	Ready      Phase = "ready"
	Discarding Phase = "discarding"
)

type Profile struct {
	Version      int            `json:"version"`
	Phase        Phase          `json:"phase"`
	Provider     string         `json:"provider"`
	Model        string         `json:"model"`
	Port         int            `json:"port"`
	BobExtension string         `json:"bob_extension,omitempty"`
	Principal    auth.Principal `json:"principal"`
}

var ErrRepair = errors.New("onboarding state requires repair")
var (
	ErrMissing   = fmt.Errorf("onboarding credential or key is missing: %w", ErrRepair)
	ErrMismatch  = fmt.Errorf("onboarding profile, credential, or key does not match: %w", ErrRepair)
	ErrExpired   = fmt.Errorf("onboarding key has expired: %w", ErrRepair)
	ErrExhausted = fmt.Errorf("onboarding key request allowance is exhausted: %w", ErrRepair)
)

// Validate checks identity before any repair can act on the recorded key.
func (p Profile) Validate() error {
	if p.Version != 1 || (p.Phase != Pending && p.Phase != Ready && p.Phase != Discarding) || p.Port < 1 || p.Port > 65535 || !auth.ModelPattern.MatchString(p.Model) || !strings.HasPrefix(p.Model, p.Provider+"/") || p.Provider == "" {
		return ErrRepair
	}
	switch p.Provider {
	case "codex", "antigravity", "gemini", "claude", "opencode":
		if p.BobExtension != "" {
			return ErrRepair
		}
	case "ibm-bob":
		if !filepath.IsAbs(p.BobExtension) {
			return ErrRepair
		}
	default:
		return ErrRepair
	}
	principal := p.Principal
	if len(principal.ID) != 32 || strings.Trim(principal.ID, "0123456789abcdef") != "" || principal.Name != "onboarding" || principal.CreatedAt.IsZero() || !reflect.DeepEqual(principal.Models, []string{p.Model}) || !reflect.DeepEqual(principal.Permissions, []string{"models:read", "completions:create"}) {
		return ErrRepair
	}
	policy := principal.Policy
	if policy.ExpiresAt == nil || !policy.ExpiresAt.Equal(principal.CreatedAt.Add(time.Hour)) || policy.RequestTimeout != auth.Duration(90*time.Second) || policy.MaxRequests != 20 || policy.RequestsPerMinute != 5 || policy.MaxConcurrent != 1 {
		return ErrRepair
	}
	return nil
}

// Resume verifies immutable identity and existing durable usage without writing.
// No recovery path creates a key when the record or credential is missing.
func Resume(store *auth.Store, p Profile, token string) (auth.Inspection, error) {
	if err := p.Validate(); err != nil || p.Phase == Discarding {
		return auth.Inspection{}, ErrMismatch
	}
	if token == "" {
		return auth.Inspection{}, ErrMissing
	}
	inspection, err := store.Inspect(p.Principal.ID)
	if errors.Is(err, os.ErrNotExist) {
		return auth.Inspection{}, ErrMissing
	}
	if err != nil || !reflect.DeepEqual(inspection.Principal, p.Principal) {
		return auth.Inspection{}, ErrMismatch
	}
	if inspection.Expired {
		return auth.Inspection{}, ErrExpired
	}
	if inspection.RemainingRequests == nil {
		return auth.Inspection{}, ErrMismatch
	}
	if *inspection.RemainingRequests == 0 {
		return auth.Inspection{}, ErrExhausted
	}
	principal, err := store.Authenticate(token)
	if errors.Is(err, auth.ErrExpiredKey) {
		return auth.Inspection{}, ErrExpired
	}
	if err != nil || !reflect.DeepEqual(principal, p.Principal) {
		return auth.Inspection{}, ErrMismatch
	}
	return inspection, nil
}

// VerifyRepair rejects conflicting identities while permitting cleanup after
// revocation, expiry, exhaustion, or interrupted credential removal.
func VerifyRepair(store *auth.Store, p Profile, token string) error {
	if p.Validate() != nil || (token != "" && (!credentialPattern.MatchString(token) || !strings.HasPrefix(token, "h2a_"+p.Principal.ID+"_"))) {
		return ErrMismatch
	}
	inspection, err := store.Inspect(p.Principal.ID)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil || !reflect.DeepEqual(inspection.Principal, p.Principal) {
		return ErrMismatch
	}
	if token != "" {
		principal, err := store.Authenticate(token)
		if errors.Is(err, auth.ErrExpiredKey) {
			return nil
		}
		if err != nil || !reflect.DeepEqual(principal, p.Principal) {
			return ErrMismatch
		}
	}
	return nil
}
