package harness

import "context"

type Usage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
	TotalTokens      int `json:"total_tokens"`
}
type Request struct {
	Model  string
	Prompt string
}

// Run emits only assistant text. Success means a terminal completion was observed.
// Implementations must stop on context cancellation and on an emit error.
type Adapter interface {
	ID() string
	Run(context.Context, Request, func(string) error) (*Usage, error)
}
