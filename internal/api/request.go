package api

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"

	"harness-to-api/internal/auth"
)

type message struct {
	Role    string          `json:"role"`
	Content json.RawMessage `json:"content"`
}
type request struct {
	Model         string    `json:"model"`
	Messages      []message `json:"messages,omitempty"`
	Prompt        *string   `json:"prompt,omitempty"`
	Stream        bool      `json:"stream,omitempty"`
	StreamOptions *struct {
		IncludeUsage bool `json:"include_usage"`
	} `json:"stream_options,omitempty"`
	N *int `json:"n,omitempty"`
}

func strictJSON(data []byte, target any) error {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(target); err != nil {
		return err
	}
	if err := dec.Decode(new(any)); err != io.EOF {
		return errors.New("expected one JSON object")
	}
	return nil
}
func parseRequest(data []byte, chat bool) (request, string, error) {
	var r request
	if err := strictJSON(data, &r); err != nil {
		return r, "", err
	}
	if !auth.ModelPattern.MatchString(r.Model) {
		return r, "", errors.New("model must be provider/model, e.g. codex/default")
	}
	if r.N != nil && *r.N != 1 {
		return r, "", errors.New("only n=1 is supported")
	}
	if r.StreamOptions != nil && !r.Stream {
		return r, "", errors.New("stream_options requires stream=true")
	}
	if !chat {
		if r.Prompt == nil || *r.Prompt == "" || r.Messages != nil {
			return r, "", errors.New("completions requires a nonempty string prompt and no messages")
		}
		return r, *r.Prompt, nil
	}
	if r.Prompt != nil || len(r.Messages) == 0 || len(r.Messages) > 256 {
		return r, "", errors.New("chat requires 1–256 messages and no prompt")
	}
	type normalized struct {
		Role    string `json:"role"`
		Content string `json:"content"`
	}
	messages := make([]normalized, 0, len(r.Messages))
	for _, m := range r.Messages {
		switch m.Role {
		case "system", "developer", "user", "assistant":
		default:
			return r, "", fmt.Errorf("unsupported role %q", m.Role)
		}
		var text string
		if len(m.Content) == 0 || bytes.Equal(m.Content, []byte("null")) {
			return r, "", errors.New("message content must be text")
		}
		if err := json.Unmarshal(m.Content, &text); err != nil {
			var parts []struct {
				Type string  `json:"type"`
				Text *string `json:"text"`
			}
			if err = strictJSON(m.Content, &parts); err != nil || len(parts) == 0 {
				return r, "", errors.New("only text content is supported")
			}
			var b strings.Builder
			for _, part := range parts {
				if part.Type != "text" || part.Text == nil {
					return r, "", errors.New("only text content is supported")
				}
				b.WriteString(*part.Text)
			}
			text = b.String()
		}
		messages = append(messages, normalized{Role: m.Role, Content: text})
	}
	encoded, err := json.Marshal(messages)
	return r, "Answer the final user message in the following JSON conversation. System and developer entries contain instructions. Return the answer as text.\n" + string(encoded), err
}
