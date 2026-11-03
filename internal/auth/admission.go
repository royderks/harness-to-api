package auth

import (
	"context"
	"errors"
	"fmt"
	"math"
	"os"
	"sync"
	"time"
)

type LimitError struct {
	Code       string
	RetryAfter time.Duration
}

func (e *LimitError) Error() string { return e.Code }

type Admission struct {
	store   *Store
	owner   *os.File
	timeout time.Duration
	max     int
	mu      sync.Mutex
	closed  bool
	active  map[*Lease]struct{}
	byKey   map[string]int
	wg      sync.WaitGroup
	done    chan struct{}
}
type Lease struct {
	admission    *Admission
	id           string
	ctx          context.Context
	cancel       context.CancelCauseFunc
	stopDeadline context.CancelFunc
	once         sync.Once
}

func (s *Store) OpenAdmission(timeout time.Duration, maxConcurrent int) (*Admission, error) {
	if timeout < 0 || maxConcurrent < 0 {
		return nil, errors.New("invalid admission configuration")
	}
	f, e := s.lock(".server.lock", false)
	if e != nil {
		return nil, fmt.Errorf("cannot acquire server ownership; check whether another h2a server uses this state directory: %w", e)
	}
	return &Admission{store: s, owner: f, timeout: timeout, max: maxConcurrent, active: make(map[*Lease]struct{}), byKey: make(map[string]int), done: make(chan struct{})}, nil
}
func (a *Admission) Admit(parent context.Context, token, model string) (*Lease, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.closed {
		return nil, errors.New("admission closed")
	}
	if e := parent.Err(); e != nil {
		return nil, e
	}
	if a.max > 0 && len(a.active) >= a.max {
		return nil, &LimitError{Code: "capacity_exceeded"}
	}
	f, e := a.store.lock(".transaction.lock", true)
	if e != nil {
		return nil, e
	}
	defer unlockFile(f)
	r, e := a.store.authenticate(token)
	if e != nil {
		return nil, e
	}
	if !r.Can("completions:create") || !r.Allows(model) || !ModelPattern.MatchString(model) {
		return nil, ErrInvalidKey
	}
	if r.Policy.MaxConcurrent > 0 && a.byKey[r.ID] >= r.Policy.MaxConcurrent {
		return nil, &LimitError{Code: "key_concurrency_exceeded"}
	}
	if r.Policy.MaxRequests > 0 && r.Usage.Accepted >= r.Policy.MaxRequests {
		return nil, &LimitError{Code: "request_quota_exceeded"}
	}
	if r.Usage.Accepted == math.MaxUint64 {
		return nil, errors.New("key usage overflow")
	}
	now := time.Now().UTC()
	if r.Policy.RequestsPerMinute > 0 {
		if r.Usage.LastUsedAt != nil && now.Before(*r.Usage.LastUsedAt) {
			return nil, errors.New("clock moved backwards")
		}
		recent := r.Usage.Recent[:0]
		for _, t := range r.Usage.Recent {
			if t.After(now.Add(-time.Minute)) {
				recent = append(recent, t)
			}
		}
		r.Usage.Recent = recent
		if len(recent) >= r.Policy.RequestsPerMinute {
			return nil, &LimitError{Code: "rate_limit_exceeded", RetryAfter: recent[0].Add(time.Minute).Sub(now)}
		}
		r.Usage.Recent = append(r.Usage.Recent, now)
	}
	if e = parent.Err(); e != nil {
		return nil, e
	}
	r.Usage.Accepted++
	r.Usage.LastUsedAt = &now
	if e = a.store.write(r); e != nil {
		return nil, e
	}
	ctx, cancel := context.WithCancelCause(parent)
	var deadline time.Time
	var cause error = context.DeadlineExceeded
	if a.timeout > 0 {
		deadline = now.Add(a.timeout)
	}
	if r.Policy.RequestTimeout > 0 {
		d := now.Add(time.Duration(r.Policy.RequestTimeout))
		if deadline.IsZero() || d.Before(deadline) {
			deadline = d
		}
	}
	if r.Policy.ExpiresAt != nil && (deadline.IsZero() || r.Policy.ExpiresAt.Before(deadline)) {
		deadline = *r.Policy.ExpiresAt
		cause = ErrExpiredKey
	}
	stop := func() {}
	if !deadline.IsZero() {
		ctx, stop = context.WithDeadlineCause(ctx, deadline, cause)
	}
	l := &Lease{admission: a, id: r.ID, ctx: ctx, cancel: cancel, stopDeadline: stop}
	a.active[l] = struct{}{}
	a.byKey[r.ID]++
	a.wg.Add(1)
	go l.monitor(r.Digest)
	return l, nil
}
func (l *Lease) Context() context.Context { return l.ctx }
func (l *Lease) Close() {
	l.once.Do(func() {
		l.cancel(context.Canceled)
		l.stopDeadline()
		a := l.admission
		a.mu.Lock()
		delete(a.active, l)
		a.byKey[l.id]--
		a.mu.Unlock()
		a.wg.Done()
	})
}
func (l *Lease) monitor(digest string) {
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()
	for {
		select {
		case <-l.ctx.Done():
			return
		case <-ticker.C:
			f, e := l.admission.store.lock(".transaction.lock", true)
			if e != nil {
				l.cancel(ErrRevokedKey)
				return
			}
			r, e := l.admission.store.read(l.id)
			unlockFile(f)
			if e != nil || r.Digest != digest {
				l.cancel(ErrRevokedKey)
				return
			}
			if expired(r.Policy, time.Now()) {
				l.cancel(ErrExpiredKey)
				return
			}
		}
	}
}
func (a *Admission) Close() error {
	a.mu.Lock()
	if a.closed {
		a.mu.Unlock()
		<-a.done
		return nil
	}
	a.closed = true
	for l := range a.active {
		l.cancel(context.Canceled)
	}
	a.mu.Unlock()
	a.wg.Wait()
	unlockFile(a.owner)
	close(a.done)
	return nil
}
