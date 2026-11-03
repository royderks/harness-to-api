package api

import (
	"fmt"
	"strings"

	"harness-to-api/internal/auth"
)

// ModelError describes a local model configuration or authorization failure.
type ModelError struct{ Code, Message string }

func (e *ModelError) Error() string { return e.Message }

// ValidateModels validates an explicit server catalog against enabled providers.
func ValidateModels(allowed []string, providers []string) error {
	for _, model := range allowed {
		if !auth.ModelPattern.MatchString(model) || strings.Contains(model, "*") {
			return &ModelError{"invalid_model", fmt.Sprintf("invalid configured model %q", model)}
		}
		provider, name, _ := strings.Cut(model, "/")
		if name == "default" || strings.HasSuffix(name, "/default") {
			return &ModelError{"invalid_model", fmt.Sprintf("dynamic default model %q is not allowed in a strict catalog", model)}
		}
		if !contains(providers, provider) {
			return &ModelError{"model_not_found", fmt.Sprintf("provider %q is not enabled", provider)}
		}
	}
	return nil
}

// CheckModel checks syntax, key scopes and the server's local model configuration.
func CheckModel(model string, scopes []string, providers []string, allowed []string) error {
	if !auth.ModelPattern.MatchString(model) || strings.Contains(model, "*") {
		return &ModelError{"invalid_model", "model must be provider/model"}
	}
	if !(auth.Principal{Models: scopes}).Allows(model) {
		return &ModelError{"permission_denied", "key cannot use this model"}
	}
	provider, _, _ := strings.Cut(model, "/")
	if !contains(providers, provider) {
		return &ModelError{"model_not_found", "provider is not enabled"}
	}
	if len(allowed) > 0 && !contains(allowed, model) {
		return &ModelError{"model_not_found", "model is not in the configured catalog"}
	}
	return nil
}
func contains(values []string, value string) bool {
	for _, v := range values {
		if v == value {
			return true
		}
	}
	return false
}
