package initprofile

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

const maxProfileBytes = 64 << 10

var credentialPattern = regexp.MustCompile(`^h2a_[a-f0-9]{32}_[A-Za-z0-9_-]{43}$`)

// Store owns the protected onboarding directory. Callers serialize writes using
// the server admission lock; Open itself must precede the acquisition of it.
type Store struct {
	dir      string
	platform platformStore
}

func Open(stateDir string) (*Store, error) {
	dir, p, err := openPrivateState(stateDir)
	if err != nil {
		return nil, err
	}
	return &Store{dir: dir, platform: p}, nil
}
func (s *Store) Close() error           { return s.platform.close() }
func (s *Store) CredentialPath() string { return filepath.Join(s.dir, "client.key") }

func (s *Store) Load() (*Profile, string, error) {
	profileBytes, profileErr := s.read("profile.json", maxProfileBytes)
	credential, credentialErr := s.read("client.key", 256)
	if profileErr != nil && !errors.Is(profileErr, os.ErrNotExist) {
		return nil, "", profileErr
	}
	if credentialErr != nil && !errors.Is(credentialErr, os.ErrNotExist) {
		return nil, "", credentialErr
	}
	if errors.Is(profileErr, os.ErrNotExist) {
		if errors.Is(credentialErr, os.ErrNotExist) {
			return nil, "", nil
		}
		return nil, "", ErrRepair
	}
	var p Profile
	d := json.NewDecoder(bytes.NewReader(profileBytes))
	d.DisallowUnknownFields()
	if d.Decode(&p) != nil || d.Decode(new(any)) != io.EOF || p.Validate() != nil {
		return nil, "", ErrRepair
	}
	if errors.Is(credentialErr, os.ErrNotExist) {
		return &p, "", nil
	}
	token := string(credential)
	if !credentialPattern.MatchString(token) || !strings.HasPrefix(token, "h2a_"+p.Principal.ID+"_") {
		return nil, "", ErrRepair
	}
	return &p, token, nil
}
func (s *Store) read(name string, max int64) ([]byte, error) {
	f, err := s.platform.openRead(s.dir, name)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	b, err := io.ReadAll(io.LimitReader(f, max+1))
	if err != nil {
		return nil, err
	}
	if int64(len(b)) > max {
		return nil, ErrRepair
	}
	return b, nil
}
func (s *Store) SaveProfile(p Profile) error {
	if err := p.Validate(); err != nil {
		return err
	}
	b, err := json.Marshal(p)
	if err != nil {
		return err
	}
	return s.publish("profile.json", b, false)
}
func (s *Store) SaveCredential(token string) error {
	if !credentialPattern.MatchString(token) {
		return ErrRepair
	}
	return s.publish("client.key", []byte(token), true)
}
func (s *Store) publish(name string, b []byte, exclusive bool) error {
	var random [16]byte
	if _, err := rand.Read(random[:]); err != nil {
		return err
	}
	temp := ".tmp-" + hex.EncodeToString(random[:])
	f, err := s.platform.create(s.dir, temp)
	if err != nil {
		return err
	}
	defer s.platform.removeTemp(s.dir, temp)
	if _, err = f.Write(b); err == nil {
		err = f.Sync()
	}
	if closeErr := f.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	return s.platform.publish(s.dir, temp, name, exclusive)
}
func (s *Store) RemoveCredential() error { return s.platform.remove(s.dir, "client.key") }
func (s *Store) RemoveProfile() error    { return s.platform.remove(s.dir, "profile.json") }
