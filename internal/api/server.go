package api

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"math"
	"mime"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"time"

	"harness-to-api/internal/auth"
	"harness-to-api/internal/harness"
)

type Config struct {
	Keys          *auth.Store
	Adapters      []harness.Adapter
	Timeout       time.Duration
	MaxConcurrent int
	Logger        *slog.Logger
	AllowedModels []string
}
type Server struct {
	config    Config
	adapters  map[string]harness.Adapter
	admission *auth.Admission
	providers []string
}

func New(config Config) (*Server, error) {
	if config.Keys == nil {
		return nil, errors.New("key store is required")
	}
	if config.Timeout == 0 {
		config.Timeout = 2 * time.Minute
	}
	if config.MaxConcurrent == 0 {
		config.MaxConcurrent = 4
	}
	if config.Timeout < time.Millisecond || config.Timeout > time.Hour || config.MaxConcurrent < 1 || config.MaxConcurrent > 128 {
		return nil, errors.New("invalid timeout or concurrency limit")
	}
	if config.Logger == nil {
		config.Logger = slog.New(slog.NewJSONHandler(io.Discard, nil))
	}
	adapters := map[string]harness.Adapter{}
	for _, a := range config.Adapters {
		if _, ok := adapters[a.ID()]; ok {
			return nil, errors.New("duplicate adapter")
		}
		adapters[a.ID()] = a
	}
	if len(adapters) == 0 {
		return nil, errors.New("at least one adapter is required")
	}
	providers := make([]string, 0, len(adapters))
	for id := range adapters {
		providers = append(providers, id)
	}
	if err := ValidateModels(config.AllowedModels, providers); err != nil {
		return nil, err
	}
	admission, err := config.Keys.OpenAdmission(config.Timeout, config.MaxConcurrent)
	if err != nil {
		return nil, err
	}
	config.AllowedModels = append([]string(nil), config.AllowedModels...)
	return &Server{config: config, adapters: adapters, admission: admission, providers: providers}, nil
}
func (s *Server) Close() error { return s.admission.Close() }
func randomID() string         { var b [16]byte; _, _ = rand.Read(b[:]); return hex.EncodeToString(b[:]) }
func jsonResponse(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
func errorBody(status int, code, message string) any {
	kind := "invalid_request_error"
	if status >= 500 {
		kind = "server_error"
	}
	return map[string]any{"error": map[string]any{"message": message, "type": kind, "param": nil, "code": code}}
}
func fail(w http.ResponseWriter, status int, code, message string) {
	jsonResponse(w, status, errorBody(status, code, message))
}
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	requestID := randomID()
	w.Header().Set("X-Request-ID", requestID)
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	if r.URL.Path == "/healthz" && r.Method == http.MethodGet {
		jsonResponse(w, 200, map[string]string{"status": "ok"})
		return
	}
	if !strings.HasPrefix(r.URL.Path, "/v1/") {
		fail(w, 404, "not_found", "Endpoint not found.")
		return
	}
	header := strings.Fields(r.Header.Get("Authorization"))
	if len(header) != 2 || !strings.EqualFold(header[0], "Bearer") {
		w.Header().Set("WWW-Authenticate", "Bearer")
		fail(w, 401, "invalid_api_key", "A valid Bearer API key is required.")
		return
	}
	principal, err := s.config.Keys.Authenticate(header[1])
	if errors.Is(err, auth.ErrInvalidKey) {
		w.Header().Set("WWW-Authenticate", "Bearer")
		fail(w, 401, "invalid_api_key", "A valid Bearer API key is required.")
		return
	}
	if err != nil {
		fail(w, 503, "key_store_error", "Unable to read API key store.")
		return
	}
	if r.URL.Path == "/v1/models" && r.Method == http.MethodGet {
		s.models(w, principal)
		return
	}
	chat := r.URL.Path == "/v1/chat/completions"
	if !chat && r.URL.Path != "/v1/completions" {
		fail(w, 404, "not_found", "Endpoint not found.")
		return
	}
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		fail(w, 405, "method_not_allowed", "Use POST.")
		return
	}
	media, _, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || media != "application/json" {
		fail(w, 400, "invalid_content_type", "Use application/json.")
		return
	}
	data, err := io.ReadAll(http.MaxBytesReader(w, r.Body, 1024*1024))
	if err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			fail(w, 413, "body_too_large", "Request exceeds 1 MiB.")
		} else {
			fail(w, 400, "invalid_body", "Unable to read request body.")
		}
		return
	}
	req, prompt, err := parseRequest(data, chat)
	if err != nil {
		fail(w, 400, "unsupported_request", err.Error())
		return
	}
	if !principal.Can("completions:create") {
		fail(w, 403, "permission_denied", "Key cannot use this model.")
		return
	}
	if err := CheckModel(req.Model, principal.Models, s.providers, s.config.AllowedModels); err != nil {
		var modelErr *ModelError
		if errors.As(err, &modelErr) && modelErr.Code == "permission_denied" {
			fail(w, 403, modelErr.Code, "Key cannot use this model.")
		} else {
			fail(w, 404, "model_not_found", "Model is not available on this server.")
		}
		return
	}
	provider, model, _ := strings.Cut(req.Model, "/")
	adapter := s.adapters[provider]
	lease, err := s.admission.Admit(r.Context(), header[1], req.Model)
	if err != nil {
		admissionError(w, err)
		return
	}
	defer lease.Close()
	ctx := lease.Context()
	input := harness.Request{Model: model, Prompt: prompt}
	id := "cmpl-" + randomID()
	if chat {
		id = "chatcmpl-" + randomID()
	}
	created := time.Now().Unix()
	if req.Stream {
		s.stream(ctx, w, adapter, input, req, chat, id, created, requestID)
		return
	}
	var text strings.Builder
	usage, err := adapter.Run(ctx, input, func(delta string) error {
		if text.Len()+len(delta) > 8*1024*1024 {
			return errors.New("output limit exceeded")
		}
		_, err := text.WriteString(delta)
		return err
	})
	if err == nil {
		err = ctx.Err()
	}
	if err != nil {
		s.logFailure(requestID, ctx)
		status, code, message := harnessError(ctx)
		fail(w, status, code, message)
		return
	}
	choice := map[string]any{"index": 0, "finish_reason": "stop", "logprobs": nil}
	object := "text_completion"
	if chat {
		object = "chat.completion"
		choice["message"] = map[string]string{"role": "assistant", "content": text.String()}
	} else {
		choice["text"] = text.String()
	}
	response := map[string]any{"id": id, "created": created, "model": req.Model, "object": object, "choices": []any{choice}}
	if usage != nil {
		response["usage"] = usage
	}
	jsonResponse(w, 200, response)
}
func (s *Server) models(w http.ResponseWriter, p auth.Principal) {
	if !p.Can("models:read") {
		fail(w, 403, "permission_denied", "Key cannot list models.")
		return
	}
	ids := map[string]bool{}
	if len(s.config.AllowedModels) > 0 {
		for _, id := range s.config.AllowedModels {
			if p.Allows(id) {
				ids[id] = true
			}
		}
	} else {
		for id := range s.adapters {
			if p.Allows(id + "/default") {
				ids[id+"/default"] = true
			}
			for _, scope := range p.Models {
				if strings.HasPrefix(scope, id+"/") && !strings.HasSuffix(scope, "*") {
					ids[scope] = true
				}
			}
		}
	}
	names := make([]string, 0, len(ids))
	for id := range ids {
		names = append(names, id)
	}
	sort.Strings(names)
	data := []any{}
	for _, id := range names {
		provider, _, _ := strings.Cut(id, "/")
		data = append(data, map[string]any{"id": id, "object": "model", "created": 0, "owned_by": provider})
	}
	jsonResponse(w, 200, map[string]any{"object": "list", "data": data})
}
func admissionError(w http.ResponseWriter, err error) {
	var limit *auth.LimitError
	if errors.As(err, &limit) {
		seconds := int(math.Ceil(limit.RetryAfter.Seconds()))
		if seconds < 1 {
			seconds = 1
		}
		if limit.Code != "request_quota_exceeded" {
			w.Header().Set("Retry-After", strconv.Itoa(seconds))
		}
		fail(w, 429, limit.Code, "Request limit exceeded.")
		return
	}
	if errors.Is(err, auth.ErrInvalidKey) || errors.Is(err, auth.ErrRevokedKey) {
		w.Header().Set("WWW-Authenticate", "Bearer")
		fail(w, 401, "invalid_api_key", "A valid Bearer API key is required.")
		return
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		fail(w, 408, "request_canceled", "Request was canceled.")
		return
	}
	fail(w, 503, "key_store_error", "Unable to read API key store.")
}
func harnessError(ctx context.Context) (int, string, string) {
	if errors.Is(context.Cause(ctx), auth.ErrExpiredKey) || errors.Is(context.Cause(ctx), auth.ErrRevokedKey) {
		return 401, "invalid_api_key", "A valid Bearer API key is required."
	}
	if errors.Is(ctx.Err(), context.Canceled) {
		return 408, "request_canceled", "Request was canceled."
	}
	if errors.Is(ctx.Err(), context.DeadlineExceeded) {
		return 504, "harness_timeout", "Harness exceeded the request timeout."
	}
	return 502, "harness_failed", "Harness failed. Check installation, login, model access, and CLI compatibility."
}
func (s *Server) logFailure(id string, ctx context.Context) {
	_, code, _ := harnessError(ctx)
	s.config.Logger.Error("request failed", "request_id", id, "code", code)
}
func (s *Server) stream(ctx context.Context, w http.ResponseWriter, adapter harness.Adapter, input harness.Request, req request, chat bool, id string, created int64, requestID string) {
	ctx, cancel := context.WithCancel(ctx)
	type result struct {
		usage *harness.Usage
		err   error
	}
	deltas := make(chan string)
	results := make(chan result, 1)
	finished := make(chan struct{})
	go func() {
		defer close(finished)
		total := 0
		usage, err := adapter.Run(ctx, input, func(text string) error {
			total += len(text)
			if total > 8*1024*1024 {
				return errors.New("output limit exceeded")
			}
			select {
			case deltas <- text:
				return nil
			case <-ctx.Done():
				return ctx.Err()
			}
		})
		if err == nil {
			err = ctx.Err()
		}
		results <- result{usage, err}
	}()
	defer func() { cancel(); <-finished }()
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("X-Accel-Buffering", "no")
	rc := http.NewResponseController(w)
	write := func(text string) error {
		_ = rc.SetWriteDeadline(time.Now().Add(10 * time.Second))
		if _, err := io.WriteString(w, text); err != nil {
			return err
		}
		return rc.Flush()
	}
	send := func(value any) error {
		data, err := json.Marshal(value)
		if err != nil {
			return err
		}
		return write("data: " + string(data) + "\n\n")
	}
	chunk := func(delta map[string]string, finish any) map[string]any {
		object := "text_completion"
		choice := map[string]any{"index": 0, "finish_reason": finish, "logprobs": nil}
		if chat {
			object = "chat.completion.chunk"
			choice["delta"] = delta
		} else {
			choice["text"] = delta["content"]
		}
		value := map[string]any{"id": id, "created": created, "model": req.Model, "object": object, "choices": []any{choice}}
		if req.StreamOptions != nil && req.StreamOptions.IncludeUsage {
			value["usage"] = nil
		}
		return value
	}
	if chat {
		if err := send(chunk(map[string]string{"role": "assistant", "content": ""}, nil)); err != nil {
			return
		}
	}
	heartbeat := time.NewTicker(15 * time.Second)
	defer heartbeat.Stop()
	for {
		select {
		case delta := <-deltas:
			if err := send(chunk(map[string]string{"content": delta}, nil)); err != nil {
				return
			}
		case res := <-results:
			if ctx.Err() != nil {
				res.err = ctx.Err()
			}
			if res.err != nil {
				s.logFailure(requestID, ctx)
				status, code, message := harnessError(ctx)
				_ = send(errorBody(status, code, message))
				return
			}
			if err := send(chunk(map[string]string{}, "stop")); err != nil {
				return
			}
			if req.StreamOptions != nil && req.StreamOptions.IncludeUsage && res.usage != nil {
				object := "text_completion"
				if chat {
					object = "chat.completion.chunk"
				}
				if err := send(map[string]any{"id": id, "created": created, "model": req.Model, "object": object, "choices": []any{}, "usage": res.usage}); err != nil {
					return
				}
			}
			if ctx.Err() == nil {
				_ = write("data: [DONE]\n\n")
			}
			return
		case <-heartbeat.C:
			if err := write(": keep-alive\n\n"); err != nil {
				return
			}
		case <-ctx.Done():
			status, code, message := harnessError(ctx)
			_ = send(errorBody(status, code, message))
			return
		}
	}
}
