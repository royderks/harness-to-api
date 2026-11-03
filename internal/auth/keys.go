package auth

import (
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

type Duration time.Duration

func (d Duration) MarshalJSON() ([]byte, error) { return json.Marshal(time.Duration(d).String()) }
func (d *Duration) UnmarshalJSON(b []byte) error {
	var s string
	if json.Unmarshal(b, &s) != nil {
		return errors.New("invalid duration")
	}
	v, e := time.ParseDuration(s)
	if e != nil {
		return errors.New("invalid duration")
	}
	*d = Duration(v)
	return nil
}

type Policy struct {
	ExpiresAt         *time.Time `json:"expires_at,omitempty"`
	RequestTimeout    Duration   `json:"request_timeout,omitempty"`
	MaxRequests       uint64     `json:"max_requests,omitempty"`
	RequestsPerMinute int        `json:"requests_per_minute,omitempty"`
	MaxConcurrent     int        `json:"max_concurrent,omitempty"`
}

func (p *Policy) UnmarshalJSON(data []byte) error {
	type plain Policy
	if string(data) == "null" {
		return errors.New("invalid policy")
	}
	var v plain
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&v); err != nil {
		return errors.New("invalid policy")
	}
	*p = Policy(v)
	return p.validate()
}

func (p Policy) validate() error {
	if p.RequestTimeout < 0 || time.Duration(p.RequestTimeout) > time.Hour || p.RequestsPerMinute < 0 || p.RequestsPerMinute > 100000 || p.MaxConcurrent < 0 || p.MaxConcurrent > 128 || (p.ExpiresAt != nil && p.ExpiresAt.IsZero()) {
		return errors.New("invalid policy")
	}
	return nil
}

type Principal struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	CreatedAt   time.Time `json:"created_at"`
	Models      []string  `json:"models"`
	Permissions []string  `json:"permissions"`
	Policy      Policy    `json:"policy"`
}
type usage struct {
	Accepted   uint64      `json:"accepted"`
	LastUsedAt *time.Time  `json:"last_used_at,omitempty"`
	Recent     []time.Time `json:"recent,omitempty"`
}

func (u *usage) UnmarshalJSON(data []byte) error {
	type plain usage
	if string(data) == "null" {
		return errors.New("invalid key usage")
	}
	var v plain
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&v); err != nil {
		return errors.New("invalid key usage")
	}
	*u = usage(v)
	return nil
}

type record struct {
	Principal
	Digest string `json:"digest"`
	Usage  usage  `json:"usage,omitempty"`
}
type Inspection struct {
	Principal
	Accepted          uint64     `json:"accepted"`
	LastUsedAt        *time.Time `json:"last_used_at,omitempty"`
	RemainingRequests *uint64    `json:"remaining_requests,omitempty"`
	Expired           bool       `json:"expired"`
	RecentRequests    int        `json:"recent_requests"`
}
type Store struct{ Directory string }

var idPattern = regexp.MustCompile(`^[a-f0-9]{32}$`)
var tokenPattern = regexp.MustCompile(`^h2a_([a-f0-9]{32})_[A-Za-z0-9_-]{43}$`)
var ModelPattern = regexp.MustCompile(`^[a-z][a-z0-9-]*/[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127}$`)
var scopePattern = regexp.MustCompile(`^(\*|[a-z][a-z0-9-]*/(\*|[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127}))$`)
var ErrInvalidKey = errors.New("invalid API key")
var ErrExpiredKey = fmt.Errorf("expired API key: %w", ErrInvalidKey)
var ErrRevokedKey = errors.New("API key revoked")

func NewStore(stateDir string) *Store { return &Store{Directory: filepath.Join(stateDir, "keys")} }
func (p Principal) Can(permission string) bool {
	for _, v := range p.Permissions {
		if v == permission {
			return true
		}
	}
	return false
}
func (p Principal) Allows(model string) bool {
	for _, v := range p.Models {
		if v == "*" || v == model || strings.HasSuffix(v, "/*") && strings.HasPrefix(model, strings.TrimSuffix(v, "*")) {
			return true
		}
	}
	return false
}
func (s *Store) lock(name string, wait bool) (*os.File, error) {
	if err := os.MkdirAll(s.Directory, 0700); err != nil {
		return nil, err
	}
	return lockFile(filepath.Join(s.Directory, name), wait)
}
func (s *Store) read(id string) (record, error) {
	var r record
	b, e := os.ReadFile(filepath.Join(s.Directory, id+".json"))
	if e != nil {
		return r, e
	}
	decoder := json.NewDecoder(bytes.NewReader(b))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&r) != nil || decoder.Decode(new(any)) != io.EOF || r.ID != id || r.Policy.validate() != nil {
		return record{}, errors.New("invalid key record")
	}
	d, e := hex.DecodeString(r.Digest)
	if e != nil || len(d) != sha256.Size || r.Name == "" || r.CreatedAt.IsZero() || len(r.Models) == 0 {
		return record{}, errors.New("invalid key record")
	}
	for _, v := range r.Models {
		if !scopePattern.MatchString(v) {
			return record{}, errors.New("invalid key record")
		}
	}
	if len(r.Usage.Recent) > r.Policy.RequestsPerMinute && r.Policy.RequestsPerMinute > 0 || r.Policy.RequestsPerMinute == 0 && len(r.Usage.Recent) > 0 || r.Usage.Accepted < uint64(len(r.Usage.Recent)) {
		return record{}, errors.New("invalid key usage")
	}
	for i, t := range r.Usage.Recent {
		if t.IsZero() || i > 0 && t.Before(r.Usage.Recent[i-1]) {
			return record{}, errors.New("invalid key usage")
		}
	}
	if r.Usage.Accepted > 0 && (r.Usage.LastUsedAt == nil || r.Usage.LastUsedAt.IsZero()) {
		return record{}, errors.New("invalid key usage")
	}
	if len(r.Usage.Recent) > 0 && r.Usage.Recent[len(r.Usage.Recent)-1].After(*r.Usage.LastUsedAt) {
		return record{}, errors.New("invalid key usage")
	}
	return r, nil
}
func (s *Store) write(r record) error {
	b, e := json.Marshal(r)
	if e != nil {
		return e
	}
	f, e := os.CreateTemp(s.Directory, ".key-")
	if e != nil {
		return e
	}
	defer os.Remove(f.Name())
	if _, e = f.Write(b); e == nil {
		e = f.Sync()
	}
	if ce := f.Close(); e == nil {
		e = ce
	}
	if e != nil {
		return e
	}
	return replaceRecord(f.Name(), filepath.Join(s.Directory, r.ID+".json"))
}
func (s *Store) Create(name string, models []string, policy Policy) (string, Principal, error) {
	if name == "" || len(name) > 100 || len(models) == 0 {
		return "", Principal{}, errors.New("provide a name (1–100 characters) and at least one model scope")
	}
	if e := policy.validate(); e != nil {
		return "", Principal{}, e
	}
	for _, v := range models {
		if !scopePattern.MatchString(v) {
			return "", Principal{}, fmt.Errorf("invalid model scope %q", v)
		}
	}
	idb := make([]byte, 16)
	secret := make([]byte, 32)
	if _, e := rand.Read(idb); e != nil {
		return "", Principal{}, e
	}
	if _, e := rand.Read(secret); e != nil {
		return "", Principal{}, e
	}
	id := hex.EncodeToString(idb)
	token := "h2a_" + id + "_" + base64.RawURLEncoding.EncodeToString(secret)
	digest := sha256.Sum256([]byte(token))
	p := Principal{ID: id, Name: name, CreatedAt: time.Now().UTC(), Models: models, Permissions: []string{"models:read", "completions:create"}, Policy: policy}
	lock, e := s.lock(".transaction.lock", true)
	if e != nil {
		return "", Principal{}, e
	}
	defer unlockFile(lock)
	if e = s.write(record{Principal: p, Digest: hex.EncodeToString(digest[:])}); e != nil {
		return "", Principal{}, e
	}
	return token, p, nil
}
func (s *Store) authenticate(token string) (record, error) {
	m := tokenPattern.FindStringSubmatch(token)
	if m == nil {
		return record{}, ErrInvalidKey
	}
	r, e := s.read(m[1])
	if errors.Is(e, os.ErrNotExist) {
		return record{}, ErrInvalidKey
	}
	if e != nil {
		return record{}, e
	}
	d, _ := hex.DecodeString(r.Digest)
	actual := sha256.Sum256([]byte(token))
	if subtle.ConstantTimeCompare(d, actual[:]) != 1 {
		return record{}, ErrInvalidKey
	}
	if expired(r.Policy, time.Now()) {
		return record{}, ErrExpiredKey
	}
	return r, nil
}
func expired(p Policy, now time.Time) bool { return p.ExpiresAt != nil && !now.Before(*p.ExpiresAt) }
func (s *Store) Authenticate(token string) (Principal, error) {
	l, e := s.lock(".transaction.lock", true)
	if e != nil {
		return Principal{}, e
	}
	defer unlockFile(l)
	r, e := s.authenticate(token)
	return r.Principal, e
}
func (s *Store) Inspect(id string) (Inspection, error) {
	if !idPattern.MatchString(id) {
		return Inspection{}, errors.New("invalid key ID")
	}
	l, e := s.lock(".transaction.lock", true)
	if e != nil {
		return Inspection{}, e
	}
	defer unlockFile(l)
	r, e := s.read(id)
	if e != nil {
		return Inspection{}, e
	}
	i := Inspection{Principal: r.Principal, Accepted: r.Usage.Accepted, LastUsedAt: r.Usage.LastUsedAt, Expired: expired(r.Policy, time.Now())}
	for _, t := range r.Usage.Recent {
		if t.After(time.Now().Add(-time.Minute)) {
			i.RecentRequests++
		}
	}
	if r.Policy.MaxRequests > 0 {
		n := uint64(0)
		if r.Usage.Accepted < r.Policy.MaxRequests {
			n = r.Policy.MaxRequests - r.Usage.Accepted
		}
		i.RemainingRequests = &n
	}
	return i, nil
}
func (s *Store) List() ([]Principal, error) {
	l, e := s.lock(".transaction.lock", true)
	if e != nil {
		return nil, e
	}
	defer unlockFile(l)
	entries, e := os.ReadDir(s.Directory)
	if e != nil {
		return nil, e
	}
	out := []Principal{}
	for _, entry := range entries {
		id := strings.TrimSuffix(entry.Name(), ".json")
		if !strings.HasSuffix(entry.Name(), ".json") || !idPattern.MatchString(id) {
			continue
		}
		r, e := s.read(id)
		if e != nil {
			return nil, e
		}
		out = append(out, r.Principal)
	}
	return out, nil
}
func (s *Store) Revoke(id string) error {
	if !idPattern.MatchString(id) {
		return errors.New("invalid key ID")
	}
	l, e := s.lock(".transaction.lock", true)
	if e != nil {
		return e
	}
	defer unlockFile(l)
	return removeRecord(filepath.Join(s.Directory, id+".json"))
}
