use crate::auth::{self, Inspection, Principal};
use serde::{Deserialize, Serialize};
#[cfg(not(windows))]
use std::fs::File;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("unsafe onboarding storage")]
    UnsafeStorage,
    #[error("onboarding state requires repair")]
    RepairRequired,
    #[error("onboarding credential or key is missing")]
    Missing,
    #[error("onboarding identity does not match")]
    Mismatch,
    #[error("onboarding key has expired")]
    Expired,
    #[error("onboarding key request allowance is exhausted")]
    Exhausted,
    #[error("onboarding storage error")]
    Io,
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Pending,
    Ready,
    Discarding,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub phase: Phase,
    pub provider: String,
    pub model: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bob_extension: String,
    pub principal: Principal,
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        let p = &self.principal;
        let valid_provider = match self.provider.as_str() {
            "codex" | "antigravity" | "gemini" | "claude" | "opencode" => {
                self.bob_extension.is_empty()
            }
            "ibm-bob" => Path::new(&self.bob_extension).is_absolute(),
            _ => false,
        };
        if self.version != 1
            || self.port == 0
            || !valid_provider
            || !auth::valid_model(&self.model)
            || !self.model.starts_with(&format!("{}/", self.provider))
            || !auth::valid_id(&p.id)
            || p.name != "onboarding"
            || p.models != [self.model.as_str()]
            || p.permissions != ["models:read", "completions:create"]
            || p.policy.expires_at != p.created_at.checked_add_signed(chrono::Duration::hours(1))
            || p.policy.request_timeout.0 != 90_000_000_000
            || p.policy.max_requests != 20
            || p.policy.requests_per_minute != 5
            || p.policy.max_concurrent != 1
        {
            return Err(Error::RepairRequired);
        }
        Ok(())
    }
}
pub fn resume(store: &auth::Store, p: &Profile, token: &str) -> Result<Inspection> {
    if p.validate().is_err() || p.phase == Phase::Discarding {
        return Err(Error::Mismatch);
    }
    if token.is_empty() {
        return Err(Error::Missing);
    }
    let i = store.inspect(&p.principal.id).map_err(|e| {
        if matches!(e, auth::Error::NotFound) {
            Error::Missing
        } else {
            Error::Mismatch
        }
    })?;
    if i.principal != p.principal {
        return Err(Error::Mismatch);
    }
    if i.expired {
        return Err(Error::Expired);
    }
    match i.remaining_requests {
        Some(0) => return Err(Error::Exhausted),
        None => return Err(Error::Mismatch),
        _ => {}
    }
    let actual = store.authenticate(token).map_err(|e| {
        if matches!(e, auth::Error::ExpiredKey) {
            Error::Expired
        } else {
            Error::Mismatch
        }
    })?;
    if actual != p.principal {
        return Err(Error::Mismatch);
    }
    Ok(i)
}
pub enum ExplicitConsent {
    Flag,
    TerminalConfirmation,
}
pub struct DiscardConfirmed(());
impl DiscardConfirmed {
    pub fn from_cli(_: ExplicitConsent) -> Self {
        Self(())
    }
}
pub struct Store {
    directory: PathBuf,
    inner: platform::Store,
    #[cfg(not(windows))]
    keys: Arc<File>,
    #[cfg(windows)]
    keys: Arc<windows::WindowsAnchor>,
}
impl Store {
    pub fn open(state: &Path) -> Result<Self> {
        let (directory, inner, keys) = platform::Store::open(state)?;
        Ok(Self {
            directory,
            inner,
            keys,
        })
    }
    pub fn auth_store(&self) -> auth::Store {
        #[cfg(not(windows))]
        {
            auth::Store::from_directory(platform::fd_path(&self.keys), self.keys.clone())
        }
        #[cfg(windows)]
        {
            auth::Store::from_windows_anchor(self.keys.clone())
        }
    }
    pub fn credential_path(&self) -> PathBuf {
        self.directory.join("client.key")
    }
    pub fn load(&self) -> Result<Option<(Profile, String)>> {
        let p = self.inner.read("profile.json", 65536)?;
        let c = self.inner.read("client.key", 256)?;
        match (p, c) {
            (None, None) => Ok(None),
            (None, Some(_)) => Err(Error::RepairRequired),
            (Some(bytes), c) => {
                let p: Profile =
                    serde_json::from_slice(&bytes).map_err(|_| Error::RepairRequired)?;
                p.validate()?;
                let token = match c {
                    None => String::new(),
                    Some(v) => String::from_utf8(v).map_err(|_| Error::RepairRequired)?,
                };
                if !token.is_empty()
                    && (!auth::valid_token(&token) || token[4..36] != p.principal.id)
                {
                    return Err(Error::RepairRequired);
                }
                Ok(Some((p, token)))
            }
        }
    }
    pub fn save_profile(&self, p: &Profile) -> Result<()> {
        p.validate()?;
        let data = serde_json::to_vec(p).map_err(|_| Error::Io)?;
        self.inner.publish("profile.json", &data, false)
    }
    pub fn create_credential(&self, token: &str) -> Result<()> {
        if !auth::valid_token(token) {
            return Err(Error::RepairRequired);
        }
        self.inner.publish("client.key", token.as_bytes(), true)
    }
    pub fn discard(&self, auth: &auth::Store, _consent: DiscardConfirmed) -> Result<()> {
        let Some((mut p, token)) = self.load()? else {
            #[cfg(windows)]
            {
                self.inner.cleanup_tombstone("client.key")?;
                self.inner.cleanup_tombstone("profile.json")?;
            }
            return Ok(());
        };
        let t = (!token.is_empty()).then_some(token.as_str());
        auth.verify_identity(&p.principal, t)
            .map_err(|_| Error::Mismatch)?;
        p.phase = Phase::Discarding;
        self.save_profile(&p)?;
        auth.revoke_identity(&p.principal, t)
            .map_err(|_| Error::Mismatch)?;
        self.inner.remove("client.key")?;
        self.inner.remove("profile.json")?;
        Ok(())
    }
}
#[cfg(windows)]
pub(crate) mod windows;
#[cfg(windows)]
use windows as platform;
#[cfg(unix)]
mod platform;
#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;
    pub struct Store;
    impl Store {
        pub fn open(_: &Path) -> Result<(PathBuf, Self, Arc<File>)> {
            Err(Error::UnsafeStorage)
        }
        pub fn read(&self, _: &str, _: usize) -> Result<Option<Vec<u8>>> {
            Err(Error::UnsafeStorage)
        }
        pub fn publish(&self, _: &str, _: &[u8], _: bool) -> Result<()> {
            Err(Error::UnsafeStorage)
        }
        pub fn remove(&self, _: &str) -> Result<()> {
            Err(Error::UnsafeStorage)
        }
    }
    pub fn fd_path(_: &File) -> PathBuf {
        PathBuf::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    fn profile(k: &auth::PreparedKey) -> Profile {
        Profile {
            version: 1,
            phase: Phase::Pending,
            provider: "codex".into(),
            model: "codex/test".into(),
            port: 8787,
            bob_extension: String::new(),
            principal: k.principal.clone(),
        }
    }
    #[cfg(unix)]
    #[test]
    fn retained_key_directory_survives_path_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let state = parent.path().join("state");
        let files = Store::open(&state).unwrap();
        let store = files.auth_store();
        std::fs::rename(state.join("keys"), state.join("original-keys")).unwrap();
        std::fs::create_dir(state.join("keys")).unwrap();
        let key = store
            .create(auth::KeySpec {
                name: "test".into(),
                models: vec!["*".into()],
                policy: Default::default(),
            })
            .unwrap();
        assert!(
            state
                .join("original-keys")
                .join(format!("{}.json", key.principal.id))
                .exists()
        );
        assert!(
            !state
                .join("keys")
                .join(format!("{}.json", key.principal.id))
                .exists()
        );
        assert_eq!(store.authenticate(&key.token).unwrap(), key.principal);
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_state_path_fails_before_creating_any_directory() {
        use std::os::unix::ffi::OsStringExt;
        let parent = tempfile::tempdir().unwrap();
        let invalid = std::ffi::OsString::from_vec(vec![b'x', 0xff]);
        let requested = parent.path().join("missing").join(invalid).join("state");
        assert!(matches!(Store::open(&requested), Err(Error::UnsafeStorage)));
        assert!(!parent.path().join("missing").exists());
    }
    #[cfg(windows)]
    #[test]
    fn retry_interrupted_final_discard_and_create_fresh_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let s = Store::open(&state).unwrap();
        let auth = s.auth_store();
        let now = Utc::now();
        let make_key = || {
            auth::prepare(
                auth::KeySpec {
                    name: "onboarding".into(),
                    models: vec!["codex/test".into()],
                    policy: auth::Policy {
                        expires_at: Some(now + chrono::Duration::hours(1)),
                        request_timeout: auth::DurationValue(90_000_000_000),
                        max_requests: 20,
                        requests_per_minute: 5,
                        max_concurrent: 1,
                    },
                },
                now,
            )
            .unwrap()
        };
        let old = make_key();
        let mut p = profile(&old);
        s.save_profile(&p).unwrap();
        s.create_credential(&old.token).unwrap();
        auth.insert_prepared(&old).unwrap();
        let unrelated = auth
            .create(auth::KeySpec {
                name: "unrelated".into(),
                models: vec!["*".into()],
                policy: Default::default(),
            })
            .unwrap();
        p.phase = Phase::Discarding;
        s.save_profile(&p).unwrap();
        auth.revoke_identity(&p.principal, Some(&old.token))
            .unwrap();
        s.inner.remove("client.key").unwrap();
        std::fs::rename(
            state.join("init/profile.json"),
            state.join("init/.discard-profile.json"),
        )
        .unwrap();
        s.discard(&auth, DiscardConfirmed::from_cli(ExplicitConsent::Flag))
            .unwrap();
        assert!(!state.join("init/.discard-profile.json").exists());
        assert!(s.load().unwrap().is_none());
        assert_eq!(
            auth.authenticate(&unrelated.token).unwrap(),
            unrelated.principal
        );
        assert!(auth.authenticate(&old.token).is_err());
        let fresh = make_key();
        let fresh_profile = profile(&fresh);
        s.save_profile(&fresh_profile).unwrap();
        s.create_credential(&old.token).unwrap();
        let profile_before = std::fs::read(state.join("init/profile.json")).unwrap();
        assert!(
            s.discard(&auth, DiscardConfirmed::from_cli(ExplicitConsent::Flag))
                .is_err()
        );
        assert_eq!(
            std::fs::read(state.join("init/profile.json")).unwrap(),
            profile_before
        );
        assert_eq!(
            auth.authenticate(&unrelated.token).unwrap(),
            unrelated.principal
        );
        s.inner.remove("client.key").unwrap();
        s.create_credential(&fresh.token).unwrap();
        auth.insert_prepared(&fresh).unwrap();
        assert!(resume(&auth, &fresh_profile, &fresh.token).is_ok());
        s.discard(&auth, DiscardConfirmed::from_cli(ExplicitConsent::Flag))
            .unwrap();
        assert!(s.load().unwrap().is_none());
        assert_eq!(
            auth.authenticate(&unrelated.token).unwrap(),
            unrelated.principal
        );
    }
    #[test]
    fn lifecycle_and_no_resurrection() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("state")).unwrap();
        let auth = s.auth_store();
        let now = Utc::now();
        let k = auth::prepare(
            auth::KeySpec {
                name: "onboarding".into(),
                models: vec!["codex/test".into()],
                policy: auth::Policy {
                    expires_at: Some(now + chrono::Duration::hours(1)),
                    request_timeout: auth::DurationValue(90_000_000_000),
                    max_requests: 20,
                    requests_per_minute: 5,
                    max_concurrent: 1,
                },
            },
            now,
        )
        .unwrap();
        let p = profile(&k);
        s.save_profile(&p).unwrap();
        s.create_credential(&k.token).unwrap();
        assert!(matches!(resume(&auth, &p, &k.token), Err(Error::Missing)));
        auth.insert_prepared(&k).unwrap();
        assert!(resume(&auth, &p, &k.token).is_ok());
        auth.revoke(&p.principal.id).unwrap();
        assert!(matches!(resume(&auth, &p, &k.token), Err(Error::Missing)));
        s.discard(&auth, DiscardConfirmed::from_cli(ExplicitConsent::Flag))
            .unwrap();
        assert!(s.load().unwrap().is_none());
    }
}
