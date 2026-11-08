// Package initprofile defines the nonsecret onboarding journal and recovery checks.
package initprofile

import (
	"errors"
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

// Validate checks identity before any repair can act on the recorded key.
func (p Profile) Validate() error {
	if p.Version != 1 || (p.Phase != Pending && p.Phase != Ready && p.Phase != Discarding) || p.Port < 1 || p.Port > 65535 || !auth.ModelPattern.MatchString(p.Model) || !strings.HasPrefix(p.Model, p.Provider+"/") || p.Provider == "" {
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
	if err := p.Validate(); err != nil || p.Phase == Discarding || token == "" {
		return auth.Inspection{}, ErrRepair
	}
	principal, err := store.Authenticate(token)
	if err != nil || !reflect.DeepEqual(principal, p.Principal) {
		return auth.Inspection{}, ErrRepair
	}
	inspection, err := store.Inspect(p.Principal.ID)
	if err != nil || !reflect.DeepEqual(inspection.Principal, p.Principal) || inspection.Expired || inspection.RemainingRequests == nil || *inspection.RemainingRequests == 0 {
		return auth.Inspection{}, ErrRepair
	}
	return inspection, nil
}
