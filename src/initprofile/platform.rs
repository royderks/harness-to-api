use super::*;
use std::{
    ffi::CString,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::Component,
};
fn c(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::UnsafeStorage)
}
pub(super) fn safe_file(file: &File, dir: bool, private: bool) -> Result<()> {
    let m = file.metadata().map_err(|_| Error::Io)?;
    let uid = unsafe { libc::geteuid() };
    let mode = m.mode();
    if dir {
        if !m.is_dir()
            || (private && (m.uid() != uid || mode & 0o7777 != 0o700))
            || (!private
                && (m.uid() != uid && m.uid() != 0
                    || mode & 0o022 != 0 && !(m.uid() == 0 && mode & 0o1000 != 0)))
        {
            return Err(Error::UnsafeStorage);
        }
    } else if !m.is_file() || m.uid() != uid || m.nlink() != 1 || mode & 0o7777 != 0o600 {
        return Err(Error::UnsafeStorage);
    }
    if private {
        check_security(file)?
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn check_security(f: &File) -> Result<()> {
    let attrs: [u32; 6] = [5, 0x00400000, 0, 0, 0, 0];
    let mut b = [0u8; 8192];
    let rc = unsafe {
        libc::syscall(
            228,
            f.as_raw_fd(),
            attrs.as_ptr(),
            b.as_mut_ptr(),
            b.len(),
            0,
        )
    };
    if rc != 0 {
        return Err(Error::UnsafeStorage);
    }
    let size = u32::from_ne_bytes(b[0..4].try_into().unwrap()) as usize;
    let len = u32::from_ne_bytes(b[8..12].try_into().unwrap());
    if !(12..=b.len()).contains(&size) || len != 0 {
        return Err(Error::UnsafeStorage);
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn check_security(_: &File) -> Result<()> {
    Ok(())
}
fn open_at(dir: &File, name: &str, flags: i32, mode: u32) -> std::io::Result<File> {
    let name =
        CString::new(name).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
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
}
fn child(parent: &File, name: &str, private: bool) -> Result<File> {
    let name_c = c(name)?;
    let file = match open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if unsafe { libc::mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o700) } != 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(Error::Io);
            }
            open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
                .map_err(|_| Error::UnsafeStorage)?
        }
        Err(_) => return Err(Error::UnsafeStorage),
    };
    safe_file(&file, true, private)?;
    Ok(file)
}
#[cfg(target_os = "linux")]
fn local(f: &File) -> Result<()> {
    let mut v = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(f.as_raw_fd(), v.as_mut_ptr()) } != 0 {
        return Err(Error::UnsafeStorage);
    }
    let magic = unsafe { v.assume_init() }.f_type as u64;
    if ![
        0xef53, 0x58465342, 0x9123683e, 0x01021994, 0x794c7630, 0x2fc12fc1, 0x858458f6,
    ]
    .contains(&magic)
    {
        return Err(Error::UnsafeStorage);
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn local(f: &File) -> Result<()> {
    let mut v = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(f.as_raw_fd(), v.as_mut_ptr()) } != 0
        || unsafe { v.assume_init() }.f_flags & 0x1000 == 0
    {
        return Err(Error::UnsafeStorage);
    }
    Ok(())
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn local(_: &File) -> Result<()> {
    Err(Error::UnsafeStorage)
}
pub fn fd_path(f: &File) -> PathBuf {
    #[cfg(target_os = "linux")]
    let prefix = "/proc/self/fd";
    #[cfg(not(target_os = "linux"))]
    let prefix = "/dev/fd";
    PathBuf::from(format!("{prefix}/{}", f.as_raw_fd()))
}

fn names_in(dir: &File) -> Result<Vec<String>> {
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            c".".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(Error::Io);
    }
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        unsafe { libc::close(fd) };
        return Err(Error::Io);
    }
    let mut names = Vec::new();
    loop {
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }
            .to_str()
            .map_err(|_| Error::UnsafeStorage)?
            .to_owned();
        if name != "." && name != ".." {
            names.push(name)
        }
    }
    unsafe { libc::closedir(stream) };
    Ok(names)
}
pub struct Store {
    dir: File,
}
impl Store {
    pub fn open(state: &Path) -> Result<(PathBuf, Self, Arc<File>)> {
        if state.as_os_str().is_empty() {
            return Err(Error::UnsafeStorage);
        }
        #[allow(unused_mut)]
        let mut path = if state.is_absolute() {
            state.to_path_buf()
        } else {
            std::env::current_dir().map_err(|_| Error::Io)?.join(state)
        };
        if path.components().any(|v| matches!(v, Component::ParentDir)) {
            return Err(Error::UnsafeStorage);
        }
        #[cfg(target_os = "macos")]
        {
            for (alias, target) in [("/tmp", "/private/tmp"), ("/var", "/private/var")] {
                if let Ok(rest) = path.strip_prefix(alias) {
                    let m = std::fs::symlink_metadata(alias).map_err(|_| Error::UnsafeStorage)?;
                    if !m.file_type().is_symlink()
                        || m.uid() != 0
                        || std::fs::read_link(alias).map_err(|_| Error::UnsafeStorage)?
                            != Path::new(&target[1..])
                    {
                        return Err(Error::UnsafeStorage);
                    }
                    path = Path::new(target).join(rest);
                    break;
                }
            }
        }
        let names: Vec<String> = path
            .components()
            .filter_map(|v| match v {
                Component::Normal(s) => {
                    Some(s.to_str().map(String::from).ok_or(Error::UnsafeStorage))
                }
                _ => None,
            })
            .collect::<Result<_>>()?;
        if names.is_empty() {
            return Err(Error::UnsafeStorage);
        }
        let mut current = File::open("/").map_err(|_| Error::Io)?;
        for (i, name) in names.iter().enumerate() {
            current = child(&current, name, i + 1 == names.len())?;
        }
        let keys = Arc::new(child(&current, "keys", true)?);
        let init = child(&current, "init", true)?;
        local(&keys)?;
        local(&init)?;
        for name in names_in(&keys)? {
            let file = open_at(&keys, &name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
                .map_err(|_| Error::UnsafeStorage)?;
            safe_file(&file, false, true)?;
        }
        Ok((path.join("init"), Self { dir: init }, keys))
    }
    pub fn read(&self, name: &str, max: usize) -> Result<Option<Vec<u8>>> {
        let file = match open_at(&self.dir, name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::UnsafeStorage),
        };
        safe_file(&file, false, true)?;
        let mut b = Vec::new();
        file.take(max as u64 + 1)
            .read_to_end(&mut b)
            .map_err(|_| Error::Io)?;
        if b.len() > max {
            return Err(Error::RepairRequired);
        }
        Ok(Some(b))
    }
    pub fn publish(&self, name: &str, data: &[u8], exclusive: bool) -> Result<()> {
        use rand::RngCore;
        let mut random = [0u8; 16];
        rand::rng().fill_bytes(&mut random);
        let temp = format!(".tmp-{}", hex::encode(random));
        let mut f = open_at(
            &self.dir,
            &temp,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
        .map_err(|_| Error::Io)?;
        let result = (|| {
            safe_file(&f, false, true)?;
            f.write_all(data)
                .and_then(|_| f.sync_all())
                .map_err(|_| Error::Io)?;
            if !exclusive {
                let _ = self.read(name, 65536)?;
            }
            let a = c(&temp)?;
            let b = c(name)?;
            let fd = self.dir.as_raw_fd();
            #[cfg(target_os = "linux")]
            let rc = unsafe {
                libc::renameat2(
                    fd,
                    a.as_ptr(),
                    fd,
                    b.as_ptr(),
                    if exclusive { libc::RENAME_NOREPLACE } else { 0 },
                )
            };
            #[cfg(target_os = "macos")]
            let rc = unsafe {
                libc::renameatx_np(
                    fd,
                    a.as_ptr(),
                    fd,
                    b.as_ptr(),
                    if exclusive { libc::RENAME_EXCL } else { 0 },
                )
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            let rc = -1;
            if rc != 0 {
                return Err(Error::Io);
            }
            self.dir.sync_all().map_err(|_| Error::Io)
        })();
        let temp_c = c(&temp)?;
        unsafe { libc::unlinkat(self.dir.as_raw_fd(), temp_c.as_ptr(), 0) };
        result
    }
    pub fn remove(&self, name: &str) -> Result<()> {
        if self.read(name, 65536)?.is_none() {
            return Ok(());
        }
        let name = c(name)?;
        if unsafe { libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0) } != 0
            && std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound
        {
            return Err(Error::Io);
        }
        self.dir.sync_all().map_err(|_| Error::Io)
    }
}
