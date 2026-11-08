package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"harness-to-api/internal/auth"
)

func probeCommand(args []string) error {
	f := flags("probe")
	base := f.String("base-url", "http://127.0.0.1:8787/v1", "gateway base URL; HTTP requires a loopback IP")
	model := f.String("model", "", "provider/model to probe")
	if err := parse(f, args); err != nil {
		return err
	}
	token := os.Getenv("OPENAI_API_KEY")
	result, err := executeProbe(context.Background(), *base, *model, token)
	if err != nil {
		return err
	}
	if err := json.NewEncoder(os.Stdout).Encode(result); err != nil {
		return err
	}
	if !result.Available {
		return errors.New("probe failed; check gateway, credentials, model access, and limits")
	}
	return nil
}

type probeResult struct {
	CheckedAt  time.Time `json:"checked_at"`
	Model      string    `json:"model"`
	Available  bool      `json:"available"`
	HTTPStatus int       `json:"http_status,omitempty"`
}

func executeProbe(ctx context.Context, base, model, token string) (probeResult, error) {
	if !auth.ModelPattern.MatchString(model) || strings.Contains(model, "*") {
		return probeResult{}, errors.New("--model must be provider/model")
	}
	u, err := url.Parse(base)
	if err != nil || u.Host == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" || (u.Scheme != "http" && u.Scheme != "https") {
		return probeResult{}, errors.New("--base-url must be an HTTP(S) URL without credentials, query or fragment")
	}
	if u.Scheme == "http" {
		ip := net.ParseIP(u.Hostname())
		if ip == nil || !ip.IsLoopback() {
			return probeResult{}, errors.New("plain HTTP is allowed only for a loopback IP")
		}
	}
	if token == "" {
		return probeResult{}, errors.New("OPENAI_API_KEY is required")
	}
	fmt.Fprintln(os.Stderr, "Probe sends a real completion request and may consume quota.")
	u.Path = strings.TrimRight(u.Path, "/") + "/chat/completions"
	u.RawPath = ""
	body, _ := json.Marshal(map[string]any{"model": model, "messages": []map[string]string{{"role": "user", "content": "Reply with OK."}}})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, u.String(), bytes.NewReader(body))
	if err != nil {
		return probeResult{}, errors.New("invalid probe URL")
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Content-Type", "application/json")
	client := &http.Client{Timeout: 90 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	checked := time.Now().UTC()
	resp, err := client.Do(req)
	available := false
	status := 0
	if err == nil {
		status = resp.StatusCode
		data, readErr := io.ReadAll(io.LimitReader(resp.Body, 1024*1024+1))
		resp.Body.Close()
		var completion struct {
			Object  string          `json:"object"`
			Model   string          `json:"model"`
			Error   json.RawMessage `json:"error"`
			Choices []struct {
				FinishReason string                         `json:"finish_reason"`
				Message      struct{ Role, Content string } `json:"message"`
			} `json:"choices"`
		}
		available = status == http.StatusOK && readErr == nil && len(data) <= 1024*1024 && json.Unmarshal(data, &completion) == nil && len(completion.Error) == 0 && completion.Object == "chat.completion" && completion.Model == model && len(completion.Choices) == 1
		if available {
			choice := completion.Choices[0]
			available = choice.FinishReason == "stop" && choice.Message.Role == "assistant" && strings.TrimSpace(choice.Message.Content) != ""
		}
	}
	return probeResult{CheckedAt: checked, Model: model, Available: available, HTTPStatus: status}, nil
}
