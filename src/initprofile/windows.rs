//! Windows security primitives for protected onboarding storage.
//!
//! Directory handles retain the verified walk for the lifetime of the auth store.
use super::{Error, Result};
use std::{ffi::c_void, fs::File, mem, os::windows::io::AsRawHandle, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
        CopySid, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetLengthSid,
        GetSecurityDescriptorControl, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSID,
        SE_DACL_PROTECTED, TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER, TokenOwner, TokenUser,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, GetFileInformationByHandle,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

struct Handle(windows_sys::Win32::Foundation::HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

// Vec<usize> gives the SID and token information the required native alignment.
struct Sid(Vec<usize>);
impl Sid {
    unsafe fn copy(source: PSID) -> Result<Self> {
        if source.is_null() {
            return Err(Error::UnsafeStorage);
        }
        let len = unsafe { GetLengthSid(source) } as usize;
        if len == 0 {
            return Err(Error::UnsafeStorage);
        }
        let mut data = vec![0usize; len.div_ceil(mem::size_of::<usize>())];
        if unsafe { CopySid(len as u32, data.as_mut_ptr().cast(), source) } == 0 {
            return Err(Error::UnsafeStorage);
        }
        Ok(Self(data))
    }
    fn as_ptr(&self) -> PSID {
        self.0.as_ptr() as PSID
    }
}

pub(super) struct Security {
    user: Sid,
    owner: Sid,
}
impl Security {
    pub(super) fn current() -> Result<Self> {
        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(Error::UnsafeStorage);
        }
        let token = Handle(token);
        fn token_sid(token: &Handle, class: i32) -> Result<Sid> {
            let mut size = 0;
            unsafe {
                GetTokenInformation(token.0, class, ptr::null_mut(), 0, &mut size);
            }
            if size == 0 {
                return Err(Error::UnsafeStorage);
            }
            let mut buffer = vec![0usize; (size as usize).div_ceil(mem::size_of::<usize>())];
            if unsafe {
                GetTokenInformation(token.0, class, buffer.as_mut_ptr().cast(), size, &mut size)
            } == 0
            {
                return Err(Error::UnsafeStorage);
            }
            let sid = unsafe {
                if class == TokenUser {
                    (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid
                } else {
                    (*(buffer.as_ptr() as *const TOKEN_OWNER)).Owner
                }
            };
            unsafe { Sid::copy(sid) }
        }
        Ok(Self {
            user: token_sid(&token, TokenUser)?,
            owner: token_sid(&token, TokenOwner)?,
        })
    }
    /// Validate file kind, link count and owner. Private artifacts additionally
    /// require exactly one non-inherited full-control ACE for TokenUser.
    pub(super) fn check(&self, file: &File, directory: bool, private: bool) -> Result<()> {
        let handle = file.as_raw_handle();
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { mem::zeroed() };
        if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory && info.nNumberOfLinks != 1)
        {
            return Err(Error::UnsafeStorage);
        }
        let mut owner = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let security_info = OWNER_SECURITY_INFORMATION
            | if private {
                DACL_SECURITY_INFORMATION
            } else {
                0
            };
        if unsafe {
            GetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                security_info,
                &mut owner,
                ptr::null_mut(),
                if private { &mut dacl } else { ptr::null_mut() },
                ptr::null_mut(),
                &mut descriptor,
            )
        } != 0
        {
            return Err(Error::UnsafeStorage);
        }
        let _descriptor = Local(descriptor);
        if owner.is_null()
            || unsafe { EqualSid(owner, self.user.as_ptr()) } == 0
                && (private || unsafe { EqualSid(owner, self.owner.as_ptr()) } == 0)
        {
            return Err(Error::UnsafeStorage);
        }
        if !private {
            return Ok(());
        }
        if dacl.is_null() || descriptor.is_null() {
            return Err(Error::UnsafeStorage);
        }
        let mut control = 0;
        let mut revision = 0;
        let mut acl: ACL_SIZE_INFORMATION = unsafe { mem::zeroed() };
        if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
            || control & SE_DACL_PROTECTED == 0
            || unsafe {
                GetAclInformation(
                    dacl,
                    (&mut acl as *mut ACL_SIZE_INFORMATION).cast(),
                    mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            } == 0
            || acl.AceCount != 1
        {
            return Err(Error::UnsafeStorage);
        }
        let mut ace = ptr::null_mut();
        if unsafe { GetAce(dacl, 0, &mut ace) } == 0 || ace.is_null() {
            return Err(Error::UnsafeStorage);
        }
        let ace = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
        if ace.Header.AceType != 0
            || ace.Header.AceFlags != 0
            || ace.Mask != FILE_ALL_ACCESS
            || unsafe {
                EqualSid(
                    (&ace.SidStart as *const u32).cast_mut().cast(),
                    self.user.as_ptr(),
                )
            } == 0
        {
            return Err(Error::UnsafeStorage);
        }
        Ok(())
    }
}

use std::{
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::FromRawHandle},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_EXISTING,
    },
};

fn wide(path: &std::ffi::OsStr) -> Vec<u16> {
    path.encode_wide().chain(Some(0)).collect()
}
fn unsafe_io() -> std::io::Error {
    std::io::Error::other("unsafe storage")
}
fn directory(path: &Path) -> std::io::Result<File> {
    let name = wide(path.as_os_str());
    let h = unsafe {
        CreateFileW(
            name.as_ptr(),
            // Attribute-only opens do not participate in sharing checks. Listing
            // access makes the missing FILE_SHARE_DELETE protect the name.
            0x20080 | FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let f = unsafe { File::from_raw_handle(h) };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { mem::zeroed() };
    if unsafe { GetFileInformationByHandle(h, &mut info) } == 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
    {
        return Err(unsafe_io());
    }
    Ok(f)
}
fn descriptor(security: &Security) -> Result<Local> {
    let mut sid = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(security.user.as_ptr(), &mut sid) } == 0 {
        return Err(Error::UnsafeStorage);
    }
    let allocation = Local(sid.cast());
    let mut len = 0;
    unsafe {
        while *sid.add(len) != 0 {
            len += 1;
        }
    }
    let sid_text = String::from_utf16(unsafe { std::slice::from_raw_parts(sid, len) })
        .map_err(|_| Error::UnsafeStorage)?;
    drop(allocation);
    let text: Vec<u16> = format!("O:{sid_text}D:P(A;;FA;;;{sid_text})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut sd = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut sd,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(Error::UnsafeStorage);
    }
    Ok(Local(sd))
}
fn attributes(sd: &Local) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    }
}
fn ensure_dir(path: &Path, private: bool, security: &Security, sd: &Local) -> Result<File> {
    let f = match directory(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let name = wide(path.as_os_str());
            if unsafe { CreateDirectoryW(name.as_ptr(), &attributes(sd)) } == 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(Error::Io);
            }
            directory(path).map_err(|_| Error::UnsafeStorage)?
        }
        Err(_) => return Err(Error::UnsafeStorage),
    };
    if private {
        security.check(&f, true, true)?;
    }
    Ok(f)
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}
fn open_checked(
    path: &Path,
    security: &Security,
    private: bool,
    create: bool,
    exclusive: bool,
) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .write(create)
        .create(create && !exclusive)
        .create_new(exclusive)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    security
        .check(&file, false, private)
        .map_err(|_| unsafe_io())?;
    Ok(file)
}
pub(crate) struct WindowsAnchor {
    pub(crate) path: PathBuf,
    _handles: Vec<File>,
    security: Security,
}
impl WindowsAnchor {
    pub(crate) fn open_auth(
        &self,
        name: &str,
        create: bool,
        exclusive: bool,
    ) -> std::io::Result<File> {
        if !valid_name(name) {
            return Err(unsafe_io());
        }
        open_checked(
            &self.path.join(name),
            &self.security,
            false,
            create,
            exclusive,
        )
    }
}
pub struct Store {
    path: PathBuf,
    anchor: Arc<WindowsAnchor>,
}
impl Store {
    pub fn open(state: &Path) -> Result<(PathBuf, Self, Arc<WindowsAnchor>)> {
        if state.as_os_str().is_empty() {
            return Err(Error::UnsafeStorage);
        }
        let path = if state.is_absolute() {
            state.to_path_buf()
        } else {
            std::env::current_dir().map_err(|_| Error::Io)?.join(state)
        };
        let mut components = path.components();
        let drive = match components.next() {
            Some(Component::Prefix(p)) => match p.kind() {
                std::path::Prefix::Disk(_) => p.as_os_str().to_os_string(),
                _ => return Err(Error::UnsafeStorage),
            },
            _ => return Err(Error::UnsafeStorage),
        };
        if !matches!(components.next(), Some(Component::RootDir)) {
            return Err(Error::UnsafeStorage);
        }
        let parts: Vec<_> = components
            .map(|c| match c {
                Component::Normal(n) if !n.to_string_lossy().contains(':') => Ok(n.to_os_string()),
                _ => Err(Error::UnsafeStorage),
            })
            .collect::<Result<_>>()?;
        if parts.is_empty() {
            return Err(Error::UnsafeStorage);
        }
        let mut current = PathBuf::from(drive);
        current.push("\\");
        if unsafe { GetDriveTypeW(wide(current.as_os_str()).as_ptr()) } != 3 {
            return Err(Error::UnsafeStorage);
        }
        let security = Security::current()?;
        let sd = descriptor(&security)?;
        let mut handles = vec![directory(&current).map_err(|_| Error::UnsafeStorage)?];
        for (i, part) in parts.iter().enumerate() {
            current.push(part);
            handles.push(ensure_dir(&current, i + 1 == parts.len(), &security, &sd)?);
        }
        let keys = current.join("keys");
        let init = current.join("init");
        handles.push(ensure_dir(&keys, true, &security, &sd)?);
        handles.push(ensure_dir(&init, true, &security, &sd)?);
        let anchor = Arc::new(WindowsAnchor {
            path: keys,
            _handles: handles,
            security,
        });
        for entry in std::fs::read_dir(&anchor.path).map_err(|_| Error::Io)? {
            let name = entry
                .map_err(|_| Error::Io)?
                .file_name()
                .into_string()
                .map_err(|_| Error::UnsafeStorage)?;
            anchor
                .open_auth(&name, false, false)
                .map_err(|_| Error::UnsafeStorage)?;
        }
        Ok((
            init.clone(),
            Self {
                path: init,
                anchor: anchor.clone(),
            },
            anchor,
        ))
    }
    fn open_private(&self, name: &str) -> std::io::Result<File> {
        if !valid_name(name) {
            return Err(unsafe_io());
        }
        open_checked(
            &self.path.join(name),
            &self.anchor.security,
            true,
            false,
            false,
        )
    }
    pub fn read(&self, name: &str, max: usize) -> Result<Option<Vec<u8>>> {
        let file = match self.open_private(name) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::UnsafeStorage),
        };
        let mut data = Vec::new();
        file.take(max as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| Error::Io)?;
        if data.len() > max {
            return Err(Error::RepairRequired);
        }
        Ok(Some(data))
    }
    pub fn publish(&self, name: &str, data: &[u8], exclusive: bool) -> Result<()> {
        use rand::RngCore;
        if !valid_name(name) {
            return Err(Error::UnsafeStorage);
        }
        let mut random = [0u8; 16];
        rand::rng().fill_bytes(&mut random);
        let temp = self.path.join(format!(".tmp-{}", hex::encode(random)));
        let sd = descriptor(&self.anchor.security)?;
        let temp_w = wide(temp.as_os_str());
        let h = unsafe {
            CreateFileW(
                temp_w.as_ptr(),
                GENERIC_WRITE | GENERIC_READ | 0x20000,
                FILE_SHARE_READ,
                &attributes(&sd),
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(Error::Io);
        }
        let mut f = unsafe { File::from_raw_handle(h) };
        let result = (|| {
            self.anchor.security.check(&f, false, true)?;
            f.write_all(data)
                .and_then(|_| f.sync_all())
                .map_err(|_| Error::Io)?;
            drop(f);
            if !exclusive {
                let _ = self.read(name, 65536)?;
            }
            let target = wide(self.path.join(name).as_os_str());
            if unsafe {
                MoveFileExW(
                    temp_w.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_WRITE_THROUGH
                        | if exclusive {
                            0
                        } else {
                            MOVEFILE_REPLACE_EXISTING
                        },
                )
            } == 0
            {
                return Err(Error::Io);
            }
            Ok(())
        })();
        let _ = std::fs::remove_file(temp);
        result
    }
    pub fn cleanup_tombstone(&self, name: &str) -> Result<()> {
        if !valid_name(name) {
            return Err(Error::UnsafeStorage);
        }
        let tomb = format!(".discard-{name}");
        if self.read(&tomb, 65536)?.is_some() {
            std::fs::remove_file(self.path.join(&tomb)).map_err(|_| Error::Io)?;
        }
        Ok(())
    }
    pub fn remove(&self, name: &str) -> Result<()> {
        self.cleanup_tombstone(name)?;
        if self.read(name, 65536)?.is_none() {
            return Ok(());
        }
        let tomb = format!(".discard-{name}");
        let from = wide(self.path.join(name).as_os_str());
        let to = wide(self.path.join(&tomb).as_os_str());
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(Error::Io);
        }
        std::fs::remove_file(self.path.join(tomb)).map_err(|_| Error::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_store_retains_ancestors_and_publishes_exclusively() {
        let parent = tempfile::tempdir().unwrap();
        let state = parent.path().join("state");
        let (_, store, anchor) = Store::open(&state).unwrap();
        store.publish("client.key", b"first", true).unwrap();
        assert!(store.publish("client.key", b"second", true).is_err());
        assert_eq!(store.read("client.key", 256).unwrap().unwrap(), b"first");
        store.publish("profile.json", b"one", false).unwrap();
        store.publish("profile.json", b"two", false).unwrap();
        assert_eq!(store.read("profile.json", 256).unwrap().unwrap(), b"two");
        drop(store);
        assert!(std::fs::rename(&state, parent.path().join("moved")).is_err());
        assert!(std::fs::rename(state.join("keys"), state.join("other")).is_err());
        drop(anchor);
        assert!(std::fs::rename(&state, parent.path().join("moved")).is_ok());
    }
    #[test]
    fn rejects_hardlinks_and_cleans_discard_tombstone() {
        let parent = tempfile::tempdir().unwrap();
        let state = parent.path().join("state");
        let (_, store, anchor) = Store::open(&state).unwrap();
        store.publish("client.key", b"secret", true).unwrap();
        std::fs::hard_link(state.join("init/client.key"), state.join("init/other")).unwrap();
        assert!(store.read("client.key", 256).is_err());
        std::fs::remove_file(state.join("init/other")).unwrap();
        std::fs::rename(
            state.join("init/client.key"),
            state.join("init/.discard-client.key"),
        )
        .unwrap();
        store.remove("client.key").unwrap();
        assert!(!state.join("init/.discard-client.key").exists());
        let auth = anchor.open_auth("record", true, true).unwrap();
        drop(auth);
        std::fs::hard_link(state.join("keys/record"), state.join("keys/other")).unwrap();
        assert!(anchor.open_auth("record", false, false).is_err());
    }
    #[test]
    fn auth_store_keeps_walk_alive_after_onboarding_store_drops() {
        let parent = tempfile::tempdir().unwrap();
        let state = parent.path().join("state");
        let onboarding = super::super::Store::open(&state).unwrap();
        let auth = onboarding.auth_store();
        drop(onboarding);
        assert!(std::fs::rename(&state, parent.path().join("moved")).is_err());
        assert!(std::fs::rename(state.join("keys"), state.join("other")).is_err());
        let key = auth
            .create(crate::auth::KeySpec {
                name: "test".into(),
                models: vec!["*".into()],
                policy: Default::default(),
            })
            .unwrap();
        assert_eq!(auth.authenticate(&key.token).unwrap(), key.principal);
        assert!(!state.join("other").exists());
        drop(auth);
        assert!(std::fs::rename(&state, parent.path().join("moved")).is_ok());
    }
    #[test]
    fn rejects_preexisting_delete_handle_on_ancestor() {
        use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_SHARE_DELETE};
        let parent = tempfile::tempdir().unwrap();
        let state = parent.path().join("state");
        std::fs::create_dir(&state).unwrap();
        let name = wide(state.as_os_str());
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                DELETE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let handle = unsafe { File::from_raw_handle(handle) };
        assert!(directory(&state).is_err());
        drop(handle);
        assert!(directory(&state).is_ok());
    }
    #[test]
    fn rejects_unrelated_owner() {
        use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
        let text: Vec<u16> = "S-1-1-0".encode_utf16().chain(Some(0)).collect();
        let mut ptr = std::ptr::null_mut();
        assert_ne!(
            unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut ptr) },
            0
        );
        let _allocation = Local(ptr);
        let security = Security {
            user: unsafe { Sid::copy(ptr).unwrap() },
            owner: unsafe { Sid::copy(ptr).unwrap() },
        };
        let dir = tempfile::tempdir().unwrap();
        let file = File::create(dir.path().join("auth-file")).unwrap();
        assert!(security.check(&file, false, false).is_err());
        assert!(security.check(&file, false, true).is_err());
    }
    #[test]
    fn rejects_inherited_acl_and_accepts_token_owned_auth_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-file");
        let file = File::create(path).unwrap();
        let security = Security::current().unwrap();
        assert!(security.check(&file, false, false).is_ok());
        // The ordinary temporary file inherits its ACL and cannot be used as
        // a private onboarding credential.
        assert!(security.check(&file, false, true).is_err());
    }
}
