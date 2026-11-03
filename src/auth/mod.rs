use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid API key")]
    InvalidKey,
    #[error("expired API key")]
    ExpiredKey,
    #[error("API key revoked")]
    RevokedKey,
    #[error("key not found")]
    NotFound,
    #[error("invalid input")]
    InvalidInput,
    #[error("conflicting identity")]
    Conflict,
    #[error("storage error")]
    Storage,
    #[error("server ownership is already held")]
    OwnerBusy,
    #[error("{code}")]
    Limit {
        code: &'static str,
        retry_after: Duration,
    },
    #[error("canceled")]
    Canceled,
    #[error("deadline exceeded")]
    Timeout,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct DurationValue(pub i64);
impl DurationValue {
    pub fn parse(input: &str) -> Result<Self> {
        if input == "0" || input == "+0" || input == "-0" {
            return Ok(Self(0));
        }
        let (negative, s) = match input.as_bytes().first() {
            Some(b'-') => (true, &input[1..]),
            Some(b'+') => (false, &input[1..]),
            _ => (false, input),
        };
        if s.is_empty() {
            return Err(Error::InvalidInput);
        }
        let mut rest = s;
        let mut total: i128 = 0;
        while !rest.is_empty() {
            let n = rest
                .bytes()
                .take_while(|c| c.is_ascii_digit() || *c == b'.')
                .count();
            if n == 0 {
                return Err(Error::InvalidInput);
            }
            let number = &rest[..n];
            rest = &rest[n..];
            let (unit, scale) = [
                ("ns", 1i128),
                ("us", 1000),
                ("µs", 1000),
                ("μs", 1000),
                ("ms", 1_000_000),
                ("s", 1_000_000_000),
                ("m", 60_000_000_000),
                ("h", 3_600_000_000_000),
            ]
            .into_iter()
            .find(|(u, _)| rest.starts_with(u))
            .ok_or(Error::InvalidInput)?;
            rest = &rest[unit.len()..];
            let mut parts = number.split('.');
            let whole = parts.next().unwrap();
            let frac = parts.next();
            if parts.next().is_some() || (whole.is_empty() && frac.unwrap_or("").is_empty()) {
                return Err(Error::InvalidInput);
            }
            let w: i128 = if whole.is_empty() {
                0
            } else {
                whole.parse().map_err(|_| Error::InvalidInput)?
            };
            total = total
                .checked_add(w.checked_mul(scale).ok_or(Error::InvalidInput)?)
                .ok_or(Error::InvalidInput)?;
            if let Some(f) = frac {
                let mut value = 0i128;
                let mut divisor = 1i128;
                for c in f.bytes() {
                    if !c.is_ascii_digit() {
                        return Err(Error::InvalidInput);
                    }
                    if divisor < 1_000_000_000_000_000_000 {
                        value = value * 10 + i128::from(c - b'0');
                        divisor *= 10;
                    }
                }
                total = total
                    .checked_add(value * scale / divisor)
                    .ok_or(Error::InvalidInput)?;
            }
            if total > i64::MAX as i128 + i128::from(negative) {
                return Err(Error::InvalidInput);
            }
        }
        Ok(Self((if negative { -total } else { total }) as i64))
    }
    pub fn as_duration(self) -> Duration {
        Duration::from_nanos(self.0.max(0) as u64)
    }
}
impl fmt::Display for DurationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0.unsigned_abs();
        if self.0 < 0 {
            write!(f, "-")?;
        }
        if n == 0 {
            return write!(f, "0s");
        }
        if n < 1_000_000_000 {
            let (unit, scale) = if n < 1000 {
                ("ns", 1)
            } else if n < 1_000_000 {
                ("µs", 1000)
            } else {
                ("ms", 1_000_000)
            };
            let whole = n / scale;
            let rem = n % scale;
            if rem == 0 {
                return write!(f, "{whole}{unit}");
            }
            let width = scale.to_string().len() - 1;
            let frac = format!("{rem:0width$}");
            return write!(f, "{whole}.{}{unit}", frac.trim_end_matches('0'));
        }
        let secs = n / 1_000_000_000;
        let rem = n % 1_000_000_000;
        let h = secs / 3600;
        let m = secs % 3600 / 60;
        let s = secs % 60;
        if h > 0 {
            write!(f, "{h}h")?;
        }
        if m > 0 || h > 0 {
            write!(f, "{m}m")?;
        }
        if rem == 0 {
            write!(f, "{s}s")
        } else {
            let frac = format!("{rem:09}");
            write!(f, "{s}.{}s", frac.trim_end_matches('0'))
        }
    }
}
impl Serialize for DurationValue {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for DurationValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
fn zero_duration(v: &DurationValue) -> bool {
    v.0 == 0
}
fn zero_u64(v: &u64) -> bool {
    *v == 0
}
fn zero_usize(v: &usize) -> bool {
    *v == 0
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "zero_duration")]
    pub request_timeout: DurationValue,
    #[serde(skip_serializing_if = "zero_u64")]
    pub max_requests: u64,
    #[serde(skip_serializing_if = "zero_usize")]
    pub requests_per_minute: usize,
    #[serde(skip_serializing_if = "zero_usize")]
    pub max_concurrent: usize,
}
impl Policy {
    fn valid(&self) -> bool {
        self.request_timeout.0 >= 0
            && self.request_timeout.0 <= 3_600_000_000_000
            && self.requests_per_minute <= 100_000
            && self.max_concurrent <= 128
            && self
                .expires_at
                .is_none_or(|v| v.timestamp() != -62135596800 || v.timestamp_subsec_nanos() != 0)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub models: Vec<String>,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub policy: Policy,
}
impl Principal {
    pub fn can(&self, p: &str) -> bool {
        self.permissions.iter().any(|v| v == p)
    }
    pub fn allows(&self, m: &str) -> bool {
        self.models.iter().any(|v| {
            v == "*"
                || v == m
                || v.strip_suffix('*')
                    .is_some_and(|prefix| v.ends_with("/*") && m.starts_with(prefix))
        })
    }
}
pub fn valid_id(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn valid_token(s: &str) -> bool {
    s.len() == 80
        && s.starts_with("h2a_")
        && valid_id(&s[4..36])
        && &s[36..37] == "_"
        && s[37..]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
pub fn valid_model(s: &str) -> bool {
    regex::Regex::new(r"^[a-z][a-z0-9-]*/[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127}$")
        .unwrap()
        .is_match(s)
}
pub fn valid_scope(s: &str) -> bool {
    s == "*"
        || regex::Regex::new(r"^[a-z][a-z0-9-]*/(\*|[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127})$")
            .unwrap()
            .is_match(s)
}
#[derive(Clone)]
pub struct KeySpec {
    pub name: String,
    pub models: Vec<String>,
    pub policy: Policy,
}
pub struct PreparedKey {
    pub principal: Principal,
    pub token: String,
}
pub fn prepare(spec: KeySpec, now: DateTime<Utc>) -> Result<PreparedKey> {
    if spec.name.is_empty()
        || spec.name.len() > 100
        || spec.models.is_empty()
        || !spec.models.iter().all(|v| valid_scope(v))
        || !spec.policy.valid()
    {
        return Err(Error::InvalidInput);
    }
    let mut id = [0u8; 16];
    let mut secret = [0u8; 32];
    rand::rng().fill_bytes(&mut id);
    rand::rng().fill_bytes(&mut secret);
    let id = hex::encode(id);
    let token = format!("h2a_{}_{}", id, URL_SAFE_NO_PAD.encode(secret));
    Ok(PreparedKey {
        principal: Principal {
            id,
            name: spec.name,
            created_at: now,
            models: spec.models,
            permissions: vec!["models:read".into(), "completions:create".into()],
            policy: spec.policy,
        },
        token,
    })
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Usage {
    accepted: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_used_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recent: Vec<DateTime<Utc>>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    #[serde(flatten)]
    principal: Principal,
    digest: String,
    #[serde(default)]
    usage: Usage,
}
#[derive(Clone, Serialize)]
pub struct Inspection {
    #[serde(flatten)]
    pub principal: Principal,
    pub accepted: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_requests: Option<u64>,
    pub expired: bool,
    pub recent_requests: usize,
}
fn zero_time(time: DateTime<Utc>) -> bool {
    time.timestamp() == -62135596800 && time.timestamp_subsec_nanos() == 0
}
fn expired(p: &Policy, now: DateTime<Utc>) -> bool {
    p.expires_at.is_some_and(|v| now >= v)
}
fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn same_digest(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}
#[derive(Clone)]
pub struct Store {
    directory: PathBuf,
    _anchor: Option<Arc<File>>,
    #[cfg(windows)]
    windows_anchor: Option<Arc<crate::initprofile::windows::WindowsAnchor>>,
}
impl Store {
    pub fn new(state: impl AsRef<Path>) -> Self {
        Self {
            directory: state.as_ref().join("keys"),
            _anchor: None,
            #[cfg(windows)]
            windows_anchor: None,
        }
    }
    #[cfg(not(windows))]
    pub(crate) fn from_directory(directory: PathBuf, anchor: Arc<File>) -> Self {
        Self {
            directory,
            _anchor: Some(anchor),
        }
    }
    #[cfg(windows)]
    pub(crate) fn from_windows_anchor(
        anchor: Arc<crate::initprofile::windows::WindowsAnchor>,
    ) -> Self {
        Self {
            directory: anchor.path.clone(),
            _anchor: None,
            windows_anchor: Some(anchor),
        }
    }
    fn lock(&self, name: &str, wait: bool) -> Result<Lock> {
        #[cfg(windows)]
        let anchored = self.windows_anchor.is_some();
        #[cfg(not(windows))]
        let anchored = self._anchor.is_some();
        if !anchored {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(&self.directory)
                .map_err(|_| Error::Storage)?;
        }
        Lock::acquire(
            self.open_file(name, true).map_err(|_| Error::Storage)?,
            wait,
        )
    }
    fn read(&self, id: &str) -> Result<Record> {
        let mut data = Vec::new();
        self.open_file(&format!("{id}.json"), false)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::NotFound
                } else {
                    Error::Storage
                }
            })?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|_| Error::Storage)?;
        if data.len() > 1024 * 1024 {
            return Err(Error::Storage);
        }
        let value: serde_json::Value = serde_json::from_slice(&data).map_err(|_| Error::Storage)?;
        let object = value.as_object().ok_or(Error::Storage)?;
        if object.keys().any(|key| {
            ![
                "id",
                "name",
                "created_at",
                "models",
                "permissions",
                "policy",
                "digest",
                "usage",
            ]
            .contains(&key.as_str())
        }) {
            return Err(Error::Storage);
        }
        let r: Record = serde_json::from_value(value).map_err(|_| Error::Storage)?;
        if r.principal.id != id
            || !valid_id(id)
            || r.principal.name.is_empty()
            || zero_time(r.principal.created_at)
            || r.principal.models.is_empty()
            || !r.principal.models.iter().all(|s| valid_scope(s))
            || !r.principal.policy.valid()
            || hex::decode(&r.digest).map_or(true, |v| v.len() != 32)
            || r.usage.accepted < r.usage.recent.len() as u64
            || (r.principal.policy.requests_per_minute == 0 && !r.usage.recent.is_empty())
            || (r.principal.policy.requests_per_minute > 0
                && r.usage.recent.len() > r.principal.policy.requests_per_minute)
            || r.usage.recent.windows(2).any(|w| w[0] > w[1])
            || r.usage.recent.iter().any(|t| zero_time(*t))
            || (r.usage.accepted > 0 && r.usage.last_used_at.is_none_or(zero_time))
            || r.usage
                .recent
                .last()
                .is_some_and(|t| r.usage.last_used_at.is_none_or(|last| *t > last))
        {
            return Err(Error::Storage);
        }
        Ok(r)
    }
    fn write(&self, r: &Record) -> Result<()> {
        let mut random = [0u8; 16];
        rand::rng().fill_bytes(&mut random);
        let temp = format!(".key-{}", hex::encode(random));
        let target = format!("{}.json", r.principal.id);
        let mut file = self.create_temp(&temp)?;
        let result = (|| {
            serde_json::to_writer(&mut file, r).map_err(|_| Error::Storage)?;
            file.flush()
                .and_then(|_| file.sync_all())
                .map_err(|_| Error::Storage)?;
            drop(file);
            self.rename_file(&temp, &target)?;
            self.sync()
        })();
        if result.is_err() {
            let _ = self.unlink(&temp);
        }
        result
    }
    pub fn insert_prepared(&self, key: &PreparedKey) -> Result<()> {
        let p = &key.principal;
        if !valid_token(&key.token)
            || key.token[4..36] != p.id
            || p.name.is_empty()
            || zero_time(p.created_at)
            || p.name.len() > 100
            || p.models.is_empty()
            || !p.models.iter().all(|v| valid_scope(v))
            || !p.policy.valid()
            || p.permissions != ["models:read", "completions:create"]
        {
            return Err(Error::InvalidInput);
        }
        let _l = self.lock(".transaction.lock", true)?;
        if self.exists(&format!("{}.json", p.id))? {
            return Err(Error::Conflict);
        }
        self.write(&Record {
            principal: p.clone(),
            digest: digest(&key.token),
            usage: Usage::default(),
        })
    }
    pub fn create(&self, spec: KeySpec) -> Result<PreparedKey> {
        let k = prepare(spec, Utc::now())?;
        self.insert_prepared(&k)?;
        Ok(k)
    }
    fn authenticate_record(&self, token: &str) -> Result<Record> {
        if !valid_token(token) {
            return Err(Error::InvalidKey);
        }
        let r = self.read(&token[4..36]).map_err(|e| {
            if matches!(e, Error::NotFound) {
                Error::InvalidKey
            } else {
                e
            }
        })?;
        if !same_digest(&r.digest, &digest(token)) {
            return Err(Error::InvalidKey);
        }
        if expired(&r.principal.policy, Utc::now()) {
            return Err(Error::ExpiredKey);
        }
        Ok(r)
    }
    pub fn authenticate(&self, token: &str) -> Result<Principal> {
        let _l = self.lock(".transaction.lock", true)?;
        Ok(self.authenticate_record(token)?.principal)
    }
    pub async fn authenticate_cancellable(
        &self,
        token: String,
        cancel: CancellationToken,
    ) -> Result<Principal> {
        let store = self.clone();
        transaction(self, &cancel, &cancel, move || {
            Ok(store.authenticate_record(&token)?.principal)
        })
        .await
    }
    pub fn inspect(&self, id: &str) -> Result<Inspection> {
        if !valid_id(id) {
            return Err(Error::InvalidInput);
        }
        let _l = self.lock(".transaction.lock", true)?;
        let r = self.read(id)?;
        let now = Utc::now();
        Ok(Inspection {
            accepted: r.usage.accepted,
            last_used_at: r.usage.last_used_at,
            remaining_requests: (r.principal.policy.max_requests > 0).then(|| {
                r.principal
                    .policy
                    .max_requests
                    .saturating_sub(r.usage.accepted)
            }),
            expired: expired(&r.principal.policy, now),
            recent_requests: r
                .usage
                .recent
                .iter()
                .filter(|t| **t > now - chrono::Duration::seconds(60))
                .count(),
            principal: r.principal,
        })
    }
    pub fn list(&self) -> Result<Vec<Principal>> {
        let _l = self.lock(".transaction.lock", true)?;
        let mut out = Vec::new();
        for name in self.names()? {
            if let Some(id) = name.strip_suffix(".json").filter(|s| valid_id(s)) {
                out.push(self.read(id)?.principal)
            }
        }
        Ok(out)
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        if !valid_id(id) {
            return Err(Error::InvalidInput);
        }
        let _l = self.lock(".transaction.lock", true)?;
        self.remove(id)
    }
    fn remove(&self, id: &str) -> Result<()> {
        self.unlink(&format!("{id}.json"))?;
        self.sync()
    }
    fn check_identity(&self, p: &Principal, token: Option<&str>) -> Result<bool> {
        if !valid_id(&p.id) || token.is_some_and(|t| !valid_token(t) || t[4..36] != p.id) {
            return Err(Error::Conflict);
        }
        match self.read(&p.id) {
            Err(Error::NotFound) => Ok(false),
            Err(e) => Err(e),
            Ok(r) => {
                if r.principal != *p || token.is_some_and(|t| !same_digest(&r.digest, &digest(t))) {
                    Err(Error::Conflict)
                } else {
                    Ok(true)
                }
            }
        }
    }
    pub fn verify_identity(&self, p: &Principal, token: Option<&str>) -> Result<()> {
        let _l = self.lock(".transaction.lock", true)?;
        self.check_identity(p, token).map(|_| ())
    }
    pub fn revoke_identity(&self, p: &Principal, token: Option<&str>) -> Result<()> {
        let _l = self.lock(".transaction.lock", true)?;
        if self.check_identity(p, token)? {
            self.remove(&p.id)?
        }
        Ok(())
    }
    pub fn open_admission(&self, timeout: Duration, max: usize) -> Result<Arc<Admission>> {
        let owner = self.lock(".server.lock", false)?;
        Ok(Arc::new(Admission {
            store: self.clone(),
            owner: Mutex::new(Some(owner)),
            timeout,
            max,
            state: Mutex::new(AdmissionState::default()),
            notify: tokio::sync::Notify::new(),
            closed: CancellationToken::new(),
        }))
    }
}
#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Storage)
}

struct Lock(File);
#[cfg(unix)]
impl Lock {
    fn acquire(f: File, wait: bool) -> Result<Self> {
        use std::os::fd::AsRawFd;
        let flags = libc::LOCK_EX | if wait { 0 } else { libc::LOCK_NB };
        if unsafe { libc::flock(f.as_raw_fd(), flags) } != 0 {
            return Err(
                if !wait && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock
                {
                    Error::OwnerBusy
                } else {
                    Error::Storage
                },
            );
        }
        Ok(Self(f))
    }
}
#[cfg(unix)]
impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
#[cfg(windows)]
impl Lock {
    fn acquire(file: File, wait: bool) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::ERROR_LOCK_VIOLATION,
            Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
            System::IO::OVERLAPPED,
        };
        let mut overlap: OVERLAPPED = unsafe { std::mem::zeroed() };
        let flags = LOCKFILE_EXCLUSIVE_LOCK | if wait { 0 } else { LOCKFILE_FAIL_IMMEDIATELY };
        if unsafe { LockFileEx(file.as_raw_handle(), flags, 0, 1, 0, &mut overlap) } == 0 {
            return Err(
                if !wait
                    && std::io::Error::last_os_error().raw_os_error()
                        == Some(ERROR_LOCK_VIOLATION as i32)
                {
                    Error::OwnerBusy
                } else {
                    Error::Storage
                },
            );
        }
        Ok(Self(file))
    }
}
#[cfg(windows)]
impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Storage::FileSystem::UnlockFileEx, System::IO::OVERLAPPED};
        let mut overlap: OVERLAPPED = unsafe { std::mem::zeroed() };
        unsafe { UnlockFileEx(self.0.as_raw_handle(), 0, 1, 0, &mut overlap) };
    }
}
#[cfg(not(any(unix, windows)))]
impl Lock {
    fn acquire(_file: File, _wait: bool) -> Result<Self> {
        Err(Error::Storage)
    }
}
struct ActiveLease {
    key: String,
    cancel: CancellationToken,
    cause: Arc<Mutex<Option<Error>>>,
}
#[derive(Default)]
struct AdmissionState {
    closed: bool,
    next: u64,
    active: HashMap<u64, ActiveLease>,
    by_key: HashMap<String, usize>,
}
pub struct Admission {
    store: Store,
    owner: Mutex<Option<Lock>>,
    timeout: Duration,
    max: usize,
    state: Mutex<AdmissionState>,
    notify: tokio::sync::Notify,
    closed: CancellationToken,
}
struct CancelGuard(Option<CancellationToken>);
impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Some(token) = self.0.take() {
            token.cancel();
        }
    }
}
pub struct Lease {
    admission: Arc<Admission>,
    id: u64,
    cancel: CancellationToken,
    cause: Arc<Mutex<Option<Error>>>,
}
fn cancel_with(token: &CancellationToken, cause: &Mutex<Option<Error>>, error: Error) {
    let mut c = cause.lock().unwrap();
    if c.is_none() {
        *c = Some(error);
        token.cancel();
    }
}
impl Admission {
    pub async fn admit(
        self: &Arc<Self>,
        token: String,
        model: String,
        parent: CancellationToken,
    ) -> Result<Lease> {
        if parent.is_cancelled() {
            return Err(Error::Canceled);
        }
        {
            let state = self.state.lock().unwrap();
            if state.closed {
                return Err(Error::Canceled);
            }
            if self.max > 0 && state.active.len() >= self.max {
                return Err(Error::Limit {
                    code: "capacity_exceeded",
                    retry_after: Duration::ZERO,
                });
            }
        }
        let child = parent.child_token();
        let mut guard = CancelGuard(Some(child.clone()));
        let a = self.clone();
        let operation_child = child.clone();
        let result = transaction(&self.store, &child, &self.closed, move || {
            a.admit_blocking(token.clone(), model.clone(), operation_child.clone())
        })
        .await;
        guard.0.take();
        result
    }
    fn admit_blocking(
        self: &Arc<Self>,
        token: String,
        model: String,
        parent: CancellationToken,
    ) -> Result<Lease> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(Error::Canceled);
        }
        if self.max > 0 && state.active.len() >= self.max {
            return Err(Error::Limit {
                code: "capacity_exceeded",
                retry_after: Duration::ZERO,
            });
        }
        if parent.is_cancelled() {
            return Err(Error::Canceled);
        }
        let mut r = self.store.authenticate_record(&token)?;
        if !r.principal.can("completions:create")
            || !valid_model(&model)
            || !r.principal.allows(&model)
        {
            return Err(Error::InvalidKey);
        }
        let p = &r.principal.policy;
        if p.max_concurrent > 0
            && *state.by_key.get(&r.principal.id).unwrap_or(&0) >= p.max_concurrent
        {
            return Err(Error::Limit {
                code: "key_concurrency_exceeded",
                retry_after: Duration::ZERO,
            });
        }
        if p.max_requests > 0 && r.usage.accepted >= p.max_requests {
            return Err(Error::Limit {
                code: "request_quota_exceeded",
                retry_after: Duration::ZERO,
            });
        }
        if r.usage.accepted == u64::MAX {
            return Err(Error::Storage);
        }
        let now = Utc::now();
        if p.requests_per_minute > 0 {
            if r.usage.last_used_at.is_some_and(|t| now < t) {
                return Err(Error::Storage);
            }
            r.usage
                .recent
                .retain(|t| *t > now - chrono::Duration::seconds(60));
            if r.usage.recent.len() >= p.requests_per_minute {
                return Err(Error::Limit {
                    code: "rate_limit_exceeded",
                    retry_after: (r.usage.recent[0] + chrono::Duration::seconds(60) - now)
                        .to_std()
                        .unwrap_or_default(),
                });
            }
            r.usage.recent.push(now);
        }
        if parent.is_cancelled() {
            return Err(Error::Canceled);
        }
        r.usage.accepted += 1;
        r.usage.last_used_at = Some(now);
        self.store.write(&r)?;
        let cancel = CancellationToken::new();
        let cause = Arc::new(Mutex::new(None));
        let id = state.next;
        state.next = state.next.wrapping_add(1);
        state.active.insert(
            id,
            ActiveLease {
                key: r.principal.id.clone(),
                cancel: cancel.clone(),
                cause: cause.clone(),
            },
        );
        *state.by_key.entry(r.principal.id.clone()).or_default() += 1;
        drop(state);
        let lease = Lease {
            admission: self.clone(),
            id,
            cancel: cancel.clone(),
            cause: cause.clone(),
        };
        let store = self.store.clone();
        let key_id = r.principal.id.clone();
        let expected = r.digest.clone();
        let timeout = self.timeout;
        let policy = r.principal.policy.clone();
        let mut deadline = None;
        for duration in [timeout, policy.request_timeout.as_duration()] {
            if !duration.is_zero() {
                let end =
                    now + chrono::Duration::from_std(duration).map_err(|_| Error::InvalidInput)?;
                if deadline.as_ref().is_none_or(|(prior, _)| end < *prior) {
                    deadline = Some((end, Error::Timeout));
                }
            }
        }
        if let Some(expiry) = policy.expires_at
            && deadline.as_ref().is_none_or(|(prior, _)| expiry < *prior)
        {
            deadline = Some((expiry, Error::ExpiredKey));
        }
        let deadline = deadline.map(|(time, cause)| {
            (
                tokio::time::Instant::now() + (time - Utc::now()).to_std().unwrap_or_default(),
                cause,
            )
        });
        tokio::runtime::Handle::current().spawn(monitor(
            store,
            (key_id, expected),
            deadline,
            parent,
            cancel,
            cause,
        ));

        Ok(lease)
    }
    pub async fn close(&self) {
        {
            let mut s = self.state.lock().unwrap();
            s.closed = true;
            self.closed.cancel();
            for active in s.active.values() {
                cancel_with(&active.cancel, &active.cause, Error::Canceled)
            }
        }
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state.lock().unwrap().active.is_empty() {
                break;
            }
            notified.await;
        }
        self.owner.lock().unwrap().take();
    }
}
impl Lease {
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }
    pub fn cause(&self) -> Option<Error> {
        self.cause.lock().unwrap().clone()
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.cancel.cancel();
        let mut s = self.admission.state.lock().unwrap();
        if let Some(active) = s.active.remove(&self.id)
            && let Some(n) = s.by_key.get_mut(&active.key)
        {
            *n -= 1;
            if *n == 0 {
                s.by_key.remove(&active.key);
            }
        }
        drop(s);
        self.admission.notify.notify_waiters();
    }
}
#[cfg(test)]
mod tests;

#[cfg(unix)]
impl Store {
    fn at(&self, name: &str, flags: i32, mode: u32) -> std::io::Result<File> {
        use std::{
            ffi::CString,
            os::fd::{AsRawFd, FromRawFd},
        };
        if let Some(dir) = &self._anchor {
            let name = CString::new(name)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
            let fd = unsafe {
                libc::openat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    mode,
                )
            };
            if fd < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(unsafe { File::from_raw_fd(fd) })
            }
        } else {
            use std::os::unix::fs::OpenOptionsExt;
            let mut o = OpenOptions::new();
            o.read(flags & libc::O_WRONLY == 0)
                .write(flags & (libc::O_WRONLY | libc::O_RDWR) != 0)
                .create(flags & libc::O_CREAT != 0)
                .create_new(flags & libc::O_EXCL != 0)
                .mode(mode)
                .custom_flags(libc::O_NOFOLLOW);
            o.open(self.directory.join(name))
        }
    }
    fn open_file(&self, name: &str, create: bool) -> std::io::Result<File> {
        self.at(
            name,
            if create {
                libc::O_RDWR | libc::O_CREAT
            } else {
                libc::O_RDONLY
            },
            0o600,
        )
    }
    fn create_temp(&self, name: &str) -> Result<File> {
        self.at(name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL, 0o600)
            .map_err(|_| Error::Storage)
    }
    fn exists(&self, name: &str) -> Result<bool> {
        use std::{ffi::CString, os::fd::AsRawFd};
        if let Some(dir) = &self._anchor {
            let name = CString::new(name).map_err(|_| Error::Storage)?;
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } == 0
            {
                Ok(true)
            } else if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(Error::Storage)
            }
        } else {
            match fs::symlink_metadata(self.directory.join(name)) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err(Error::Storage),
            }
        }
    }
    fn rename_file(&self, old: &str, new: &str) -> Result<()> {
        use std::{ffi::CString, os::fd::AsRawFd};
        if let Some(dir) = &self._anchor {
            let a = CString::new(old).map_err(|_| Error::Storage)?;
            let b = CString::new(new).map_err(|_| Error::Storage)?;
            if unsafe { libc::renameat(dir.as_raw_fd(), a.as_ptr(), dir.as_raw_fd(), b.as_ptr()) }
                != 0
            {
                return Err(Error::Storage);
            }
            Ok(())
        } else {
            fs::rename(self.directory.join(old), self.directory.join(new))
                .map_err(|_| Error::Storage)
        }
    }
    fn unlink(&self, name: &str) -> Result<()> {
        use std::{ffi::CString, os::fd::AsRawFd};
        let res = if let Some(dir) = &self._anchor {
            let name = CString::new(name).map_err(|_| Error::Storage)?;
            if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        } else {
            fs::remove_file(self.directory.join(name))
        };
        res.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                Error::Storage
            }
        })
    }
    fn sync(&self) -> Result<()> {
        if let Some(dir) = &self._anchor {
            dir.sync_all().map_err(|_| Error::Storage)
        } else {
            sync_dir(&self.directory)
        }
    }
    fn names(&self) -> Result<Vec<String>> {
        use std::os::fd::AsRawFd;
        if let Some(dir) = &self._anchor {
            let fd = unsafe {
                libc::openat(
                    dir.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(Error::Storage);
            }
            let stream = unsafe { libc::fdopendir(fd) };
            if stream.is_null() {
                unsafe { libc::close(fd) };
                return Err(Error::Storage);
            }
            let mut names = Vec::new();
            loop {
                let entry = unsafe { libc::readdir(stream) };
                if entry.is_null() {
                    break;
                }
                let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned();
                names.push(name)
            }
            unsafe { libc::closedir(stream) };
            Ok(names)
        } else {
            fs::read_dir(&self.directory)
                .map_err(|_| Error::Storage)?
                .map(|e| {
                    e.map(|v| v.file_name().to_string_lossy().into_owned())
                        .map_err(|_| Error::Storage)
                })
                .collect()
        }
    }
}
#[cfg(not(any(unix, windows)))]
impl Store {
    fn open_file(&self, _: &str, _: bool) -> std::io::Result<File> {
        Err(std::io::Error::other("unsupported"))
    }
    fn create_temp(&self, _: &str) -> Result<File> {
        Err(Error::Storage)
    }
    fn exists(&self, _: &str) -> Result<bool> {
        Err(Error::Storage)
    }
    fn rename_file(&self, _: &str, _: &str) -> Result<()> {
        Err(Error::Storage)
    }
    fn unlink(&self, _: &str) -> Result<()> {
        Err(Error::Storage)
    }
    fn sync(&self) -> Result<()> {
        Err(Error::Storage)
    }
    fn names(&self) -> Result<Vec<String>> {
        Err(Error::Storage)
    }
}
#[cfg(windows)]
impl Store {
    fn open_file(&self, name: &str, create: bool) -> std::io::Result<File> {
        if let Some(anchor) = &self.windows_anchor {
            return anchor.open_auth(name, create, false);
        }
        OpenOptions::new()
            .read(true)
            .write(create)
            .create(create)
            .open(self.directory.join(name))
    }
    fn create_temp(&self, name: &str) -> Result<File> {
        if let Some(anchor) = &self.windows_anchor {
            return anchor
                .open_auth(name, true, true)
                .map_err(|_| Error::Storage);
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory.join(name))
            .map_err(|_| Error::Storage)
    }
    fn exists(&self, name: &str) -> Result<bool> {
        if let Some(anchor) = &self.windows_anchor {
            return match anchor.open_auth(name, false, false) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err(Error::Storage),
            };
        }
        match fs::symlink_metadata(self.directory.join(name)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(Error::Storage),
        }
    }
    fn rename_file(&self, old: &str, new: &str) -> Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        if let Some(anchor) = &self.windows_anchor {
            anchor
                .open_auth(old, false, false)
                .map_err(|_| Error::Storage)?;
            match anchor.open_auth(new, false, false) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::Storage),
            }
        }
        let from: Vec<u16> = self
            .directory
            .join(old)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let to: Vec<u16> = self
            .directory
            .join(new)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            Err(Error::Storage)
        } else {
            Ok(())
        }
    }
    fn unlink(&self, name: &str) -> Result<()> {
        let path = self.directory.join(name);
        if let Some(anchor) = &self.windows_anchor {
            anchor.open_auth(name, false, false).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::NotFound
                } else {
                    Error::Storage
                }
            })?;
        }
        if !path.exists() {
            return Err(Error::NotFound);
        }
        let mut random = [0u8; 16];
        rand::rng().fill_bytes(&mut random);
        let tomb = format!(".revoked-{}", hex::encode(random));
        self.rename_file(name, &tomb)?;
        fs::remove_file(self.directory.join(tomb)).map_err(|_| Error::Storage)
    }
    fn sync(&self) -> Result<()> {
        Ok(())
    }
    fn names(&self) -> Result<Vec<String>> {
        fs::read_dir(&self.directory)
            .map_err(|_| Error::Storage)?
            .map(|e| {
                e.map(|v| v.file_name().to_string_lossy().into_owned())
                    .map_err(|_| Error::Storage)
            })
            .collect()
    }
}

async fn transaction<T, F>(
    store: &Store,
    cancel: &CancellationToken,
    closed: &CancellationToken,
    operation: F,
) -> Result<T>
where
    T: Send + 'static,
    F: Fn() -> Result<T> + Send + Sync + 'static,
{
    let operation = Arc::new(operation);
    loop {
        let store = store.clone();
        let operation = operation.clone();
        let attempt = tokio::task::spawn_blocking(move || {
            let _lock = store.lock(".transaction.lock", false)?;
            Ok(operation())
        });
        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Canceled),
            _ = closed.cancelled() => return Err(Error::Canceled),
            result = attempt => result.map_err(|_| Error::Storage)?,
        };
        match result {
            Ok(result) => return result,
            Err(Error::OwnerBusy) => {}
            Err(error) => return Err(error),
        }
        tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Canceled),
            _ = closed.cancelled() => return Err(Error::Canceled),
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }
}

async fn monitor(
    store: Store,
    identity: (String, String),
    deadline: Option<(tokio::time::Instant, Error)>,
    parent: CancellationToken,
    cancel: CancellationToken,
    cause: Arc<Mutex<Option<Error>>>,
) {
    let expiry = async {
        match deadline {
            Some((at, reason)) => {
                tokio::time::sleep_until(at).await;
                reason
            }
            None => std::future::pending().await,
        }
    };
    let watch = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let check_store = store.clone();
            let (id, digest) = identity.clone();
            let check = transaction(&store, &cancel, &cancel, move || {
                let record = check_store.read(&id)?;
                if !same_digest(&record.digest, &digest) {
                    return Err(Error::RevokedKey);
                }
                if expired(&record.principal.policy, Utc::now()) {
                    return Err(Error::ExpiredKey);
                }
                Ok(())
            })
            .await;
            match check {
                Ok(()) => {}
                Err(Error::ExpiredKey) => return Error::ExpiredKey,
                _ => return Error::RevokedKey,
            }
        }
    };
    let reason = tokio::select! {
        _ = cancel.cancelled() => return,
        _ = parent.cancelled() => Error::Canceled,
        reason = expiry => reason,
        reason = watch => reason,
    };
    cancel_with(&cancel, &cause, reason);
}
