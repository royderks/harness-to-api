package auth

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

type Principal struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	CreatedAt   time.Time `json:"created_at"`
	Models      []string  `json:"models"`
	Permissions []string  `json:"permissions"`
}
type record struct {
	Principal
	Digest string `json:"digest"`
}
type Store struct{ Directory string }

var idPattern = regexp.MustCompile(`^[a-f0-9]{32}$`)
var tokenPattern = regexp.MustCompile(`^h2a_([a-f0-9]{32})_[A-Za-z0-9_-]{43}$`)
var ModelPattern = regexp.MustCompile(`^[a-z][a-z0-9-]*/[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127}$`)
var scopePattern = regexp.MustCompile(`^(\*|[a-z][a-z0-9-]*/(\*|[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127}))$`)
var ErrInvalidKey = errors.New("invalid API key")

func NewStore(stateDir string) *Store { return &Store{Directory: filepath.Join(stateDir, "keys")} }
func (p Principal) Can(permission string) bool {
	for _, allowed := range p.Permissions {
		if permission == allowed {
			return true
		}
	}
	return false
}
func (p Principal) Allows(model string) bool {
	for _, scope := range p.Models {
		if scope == "*" || scope == model || strings.HasSuffix(scope, "/*") && strings.HasPrefix(model, strings.TrimSuffix(scope, "*")) {
			return true
		}
	}
	return false
}
func (s *Store) Create(name string, models []string) (string, Principal, error) {
	if name == "" || len(name) > 100 || len(models) == 0 {
		return "", Principal{}, errors.New("provide a name (1–100 characters) and at least one model scope")
	}
	for _, scope := range models {
		if !scopePattern.MatchString(scope) {
			return "", Principal{}, fmt.Errorf("invalid model scope %q", scope)
		}
	}
	idBytes := make([]byte, 16)
	secret := make([]byte, 32)
	if _, err := rand.Read(idBytes); err != nil {
		return "", Principal{}, err
	}
	if _, err := rand.Read(secret); err != nil {
		return "", Principal{}, err
	}
	id := hex.EncodeToString(idBytes)
	token := "h2a_" + id + "_" + base64.RawURLEncoding.EncodeToString(secret)
	digest := sha256.Sum256([]byte(token))
	p := Principal{ID: id, Name: name, CreatedAt: time.Now().UTC(), Models: models, Permissions: []string{"models:read", "completions:create"}}
	data, err := json.Marshal(record{Principal: p, Digest: hex.EncodeToString(digest[:])})
	if err != nil {
		return "", Principal{}, err
	}
	if err = os.MkdirAll(s.Directory, 0700); err != nil {
		return "", Principal{}, err
	}
	// Each key has its own file; rename publishes a complete record to concurrent readers.
	f, err := os.CreateTemp(s.Directory, ".key-")
	if err != nil {
		return "", Principal{}, err
	}
	defer os.Remove(f.Name())
	if _, err = f.Write(data); err != nil {
		f.Close()
		return "", Principal{}, err
	}
	if err = f.Close(); err != nil {
		return "", Principal{}, err
	}
	if err = os.Rename(f.Name(), filepath.Join(s.Directory, id+".json")); err != nil {
		return "", Principal{}, err
	}
	return token, p, nil
}
func (s *Store) Authenticate(token string) (Principal, error) {
	match := tokenPattern.FindStringSubmatch(token)
	if match == nil {
		return Principal{}, ErrInvalidKey
	}
	data, err := os.ReadFile(filepath.Join(s.Directory, match[1]+".json"))
	if errors.Is(err, os.ErrNotExist) {
		return Principal{}, ErrInvalidKey
	}
	if err != nil {
		return Principal{}, err
	}
	var r record
	if err = json.Unmarshal(data, &r); err != nil {
		return Principal{}, err
	}
	expected, err := hex.DecodeString(r.Digest)
	if err != nil {
		return Principal{}, err
	}
	actual := sha256.Sum256([]byte(token))
	if r.ID != match[1] || subtle.ConstantTimeCompare(expected, actual[:]) != 1 {
		return Principal{}, ErrInvalidKey
	}
	return r.Principal, nil
}
func (s *Store) List() ([]Principal, error) {
	entries, err := os.ReadDir(s.Directory)
	if errors.Is(err, os.ErrNotExist) {
		return []Principal{}, nil
	}
	if err != nil {
		return nil, err
	}
	result := []Principal{}
	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".json") || !idPattern.MatchString(strings.TrimSuffix(entry.Name(), ".json")) {
			continue
		}
		data, err := os.ReadFile(filepath.Join(s.Directory, entry.Name()))
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, err
		}
		var r record
		if err = json.Unmarshal(data, &r); err != nil {
			return nil, err
		}
		result = append(result, r.Principal)
	}
	return result, nil
}
func (s *Store) Revoke(id string) error {
	if !idPattern.MatchString(id) {
		return errors.New("invalid key ID")
	}
	return os.Remove(filepath.Join(s.Directory, id+".json"))
}
