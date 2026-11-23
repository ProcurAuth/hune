// SPDX-License-Identifier: MIT OR Apache-2.0
//! The file layer of the volume backend (5.12, 5.13). Every access is relative to a directory
//! descriptor and uses `O_NOFOLLOW` or `AT_SYMLINK_NOFOLLOW`. This module is the only place
//! that makes, writes, renames or removes files and directories (R4).

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use hune_core::time::UtcMillis;
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, RenameFlags, Stat};
use rustix::io::Errno;

use super::faults::{FaultPoint, Faults};
use crate::store::FileSig;

/// The mode of a new file (5.20).
pub(crate) const FILE_MODE: u32 = 0o640;
/// The mode of a new directory (5.20).
pub(crate) const DIR_MODE: u32 = 0o750;

/// The type of a directory entry, read without a follow of links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FType {
    Dir,
    File,
    Symlink,
    Other,
}

impl FType {
    fn of_mode(mode: u32) -> FType {
        match FileType::from_raw_mode(mode) {
            FileType::RegularFile => FType::File,
            FileType::Directory => FType::Dir,
            FileType::Symlink => FType::Symlink,
            _ => FType::Other,
        }
    }
    /// A short text for the log reason of an ignored entry.
    pub(crate) fn describe(self) -> &'static str {
        match self {
            FType::Dir => "directory",
            FType::File => "file",
            FType::Symlink => "symbolic link",
            FType::Other => "special file",
        }
    }
}

/// One entry of a directory listing.
#[derive(Debug, Clone)]
pub(crate) struct DirItem {
    /// The name. A name that is not UTF-8 is lossy and has `name_ok` false.
    pub name: String,
    pub name_ok: bool,
    pub ftype: FType,
}

/// The attributes of one entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FileStat {
    pub sig: FileSig,
    pub mode: u32,
    pub ftype: FType,
    pub mtime: UtcMillis,
}

fn to_u64<T: TryInto<u64>>(v: T) -> u64 {
    v.try_into().unwrap_or(0)
}

impl FileStat {
    pub(crate) fn of(st: &Stat) -> FileStat {
        let mtime_ns = i128::from(st.st_mtime) * 1_000_000_000 + i128::from(st.st_mtime_nsec);
        let ctime_ns = i128::from(st.st_ctime) * 1_000_000_000 + i128::from(st.st_ctime_nsec);
        FileStat {
            sig: FileSig {
                dev: to_u64(st.st_dev),
                ino: to_u64(st.st_ino),
                size: to_u64(st.st_size),
                mtime_ns,
                ctime_ns,
            },
            mode: st.st_mode & 0o7777,
            ftype: FType::of_mode(st.st_mode),
            mtime: UtcMillis::from_millis(
                st.st_mtime * 1000 + i64::try_from(st.st_mtime_nsec).unwrap_or(0) / 1_000_000,
            ),
        }
    }
}

/// An open directory.
#[derive(Debug, Clone)]
pub(crate) struct Dir {
    fd: Arc<OwnedFd>,
    faults: Faults,
}

impl Dir {
    /// Opens the root with `O_DIRECTORY | O_NOFOLLOW` (5.12, item 2).
    pub(crate) fn open_root(path: &Path, faults: Faults) -> Result<Dir, Errno> {
        let fd =
            fs::open(path, OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())?;
        Ok(Dir { fd: Arc::new(fd), faults })
    }

    pub(crate) fn raw_fd(&self) -> &OwnedFd {
        &self.fd
    }

    /// Opens a sub-directory. A link gives `ELOOP`.
    pub(crate) fn open_dir(&self, name: &str) -> Result<Dir, Errno> {
        let fd = fs::openat(
            &*self.fd,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Dir { fd: Arc::new(fd), faults: self.faults.clone() })
    }

    pub(crate) fn mkdir(&self, name: &str) -> Result<(), Errno> {
        fs::mkdirat(&*self.fd, name, Mode::from_raw_mode(DIR_MODE))
    }

    pub(crate) fn stat_self(&self) -> Result<FileStat, Errno> {
        Ok(FileStat::of(&fs::fstat(&*self.fd)?))
    }

    /// The attributes of an entry, without a follow of a link.
    pub(crate) fn stat(&self, name: &str) -> Result<FileStat, Errno> {
        Ok(FileStat::of(&fs::statat(&*self.fd, name, AtFlags::SYMLINK_NOFOLLOW)?))
    }

    /// Tells if an entry of any type has this name.
    pub(crate) fn exists(&self, name: &str) -> Result<bool, Errno> {
        match self.stat(name) {
            Ok(_) => Ok(true),
            Err(Errno::NOENT) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Lists the entries, without `.` and `..`. The order is not defined.
    pub(crate) fn list(&self) -> Result<Vec<DirItem>, Errno> {
        let mut dir = fs::Dir::read_from(&*self.fd)?;
        let mut out = Vec::new();
        while let Some(entry) = dir.read() {
            let entry = entry?;
            let raw = entry.file_name().to_bytes();
            if raw == b"." || raw == b".." {
                continue;
            }
            let (name, name_ok) = match std::str::from_utf8(raw) {
                Ok(s) => (s.to_owned(), true),
                Err(_) => (String::from_utf8_lossy(raw).into_owned(), false),
            };
            let ftype = match entry.file_type() {
                FileType::RegularFile => FType::File,
                FileType::Directory => FType::Dir,
                FileType::Symlink => FType::Symlink,
                FileType::Unknown => match self.stat(&name) {
                    Ok(st) => st.ftype,
                    Err(Errno::NOENT) => continue,
                    Err(e) => return Err(e),
                },
                _ => FType::Other,
            };
            out.push(DirItem { name, name_ok, ftype });
        }
        Ok(out)
    }

    /// Reads a regular file: at most `limit` plus 1 bytes. A link or a special file gives
    /// `ENOENT`, so the caller treats it as absent.
    pub(crate) fn read_file(&self, name: &str, limit: usize) -> Result<(Vec<u8>, FileStat), Errno> {
        let fd = fs::openat(
            &*self.fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|e| if e == Errno::LOOP { Errno::NOENT } else { e })?;
        let st = FileStat::of(&fs::fstat(&fd)?);
        if st.ftype != FType::File {
            return Err(Errno::NOENT);
        }
        let mut bytes = Vec::new();
        let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
        File::from(fd)
            .take(cap)
            .read_to_end(&mut bytes)
            .map_err(|e| e.raw_os_error().map_or(Errno::IO, Errno::from_raw_os_error))?;
        Ok((bytes, st))
    }

    /// Opens the lock file with `O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC` and the mode 0640 (5.16).
    pub(crate) fn open_lock_file(&self, name: &str) -> Result<OwnedFd, Errno> {
        fs::openat(
            &*self.fd,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(FILE_MODE),
        )
    }

    pub(crate) fn fsync(&self) -> Result<(), Errno> {
        self.faults.check(FaultPoint::Fsync)?;
        fs::fsync(&*self.fd)
    }

    pub(crate) fn unlink(&self, name: &str) -> Result<(), Errno> {
        fs::unlinkat(&*self.fd, name, AtFlags::empty())
    }

    pub(crate) fn rmdir(&self, name: &str) -> Result<(), Errno> {
        fs::unlinkat(&*self.fd, name, AtFlags::REMOVEDIR)
    }

    /// Removes a file and ignores its absence.
    pub(crate) fn unlink_quiet(&self, name: &str) {
        let _ = self.unlink(name);
    }

    pub(crate) fn rename(&self, from: &str, to_dir: &Dir, to: &str) -> Result<(), Errno> {
        fs::renameat(&*self.fd, from, &*to_dir.fd, to)
    }

    /// `renameat2` with `RENAME_NOREPLACE` (5.15, `move_to`). When the file system refuses the
    /// flag, check that the target is absent, then call `renameat`.
    pub(crate) fn rename_noreplace(&self, from: &str, to_dir: &Dir, to: &str) -> Result<(), Errno> {
        match fs::renameat_with(&*self.fd, from, &*to_dir.fd, to, RenameFlags::NOREPLACE) {
            Err(Errno::INVAL | Errno::NOSYS | Errno::OPNOTSUPP) => {
                if to_dir.exists(to)? {
                    return Err(Errno::EXIST);
                }
                self.rename(from, to_dir, to)
            }
            other => other,
        }
    }

    /// Prepare phase of 5.13: write the bytes into a temporary file and call `fsync` on it. The
    /// name of the file is `.<final name>.tmp-<16 hex>`. `mode` is the mode of the file that the
    /// commit replaces, when there is one. Thus the mode of a file that exists stays (5.20).
    pub(crate) fn prepare(&self, final_name: &str, bytes: &[u8], mode: Option<u32>) -> Result<String, Errno> {
        let mut last = Errno::EXIST;
        for _ in 0..8 {
            let tmp = temp_name(final_name)?;
            let fd = match fs::openat(
                &*self.fd,
                tmp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(FILE_MODE),
            ) {
                Ok(fd) => fd,
                Err(Errno::EXIST) => {
                    last = Errno::EXIST;
                    continue;
                }
                Err(e) => return Err(e),
            };
            let result = self.fill(&fd, bytes, mode);
            drop(fd);
            if let Err(e) = result {
                // The file was not synced or not complete: remove it. No second `fsync`.
                self.unlink_quiet(&tmp);
                return Err(e);
            }
            return Ok(tmp);
        }
        Err(last)
    }

    fn fill(&self, fd: &OwnedFd, bytes: &[u8], mode: Option<u32>) -> Result<(), Errno> {
        let mut off = 0;
        while off < bytes.len() {
            self.faults.check(FaultPoint::Write)?;
            match rustix::io::write(fd, &bytes[off..]) {
                Ok(0) => return Err(Errno::IO),
                Ok(n) => off += n,
                Err(Errno::INTR) => {}
                Err(e) => return Err(e),
            }
        }
        if let Some(m) = mode {
            fs::fchmod(fd, Mode::from_raw_mode(m))?;
        }
        self.faults.check(FaultPoint::Fsync)?;
        fs::fsync(fd)
    }

    /// Commit phase of 5.13: rename the temporary file, then `fsync` the directory.
    pub(crate) fn commit(&self, tmp: &str, final_name: &str) -> Result<(), Errno> {
        if let Err(e) = self.rename(tmp, self, final_name) {
            self.unlink_quiet(tmp);
            return Err(e);
        }
        self.fsync()
    }

    /// Writes one file with the two phases (5.13).
    pub(crate) fn write_atomic(&self, name: &str, bytes: &[u8], mode: Option<u32>) -> Result<(), Errno> {
        let tmp = self.prepare(name, bytes, mode)?;
        self.commit(&tmp, name)
    }
}

impl Dir {
    /// The write probe of 5.21: make a file with `O_EXCL`, write, `fsync`, remove.
    pub(crate) fn probe(&self) -> Result<(), Errno> {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).map_err(|_| Errno::IO)?;
        let name = format!("probe-{:016x}", u64::from_be_bytes(b));
        let fd = fs::openat(
            &*self.fd,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(FILE_MODE),
        )?;
        let result = rustix::io::write(&fd, b"probe\n").map(|_| ()).and_then(|()| fs::fsync(&fd));
        drop(fd);
        self.unlink_quiet(&name);
        result
    }
}

fn temp_name(final_name: &str) -> Result<String, Errno> {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).map_err(|_| Errno::IO)?;
    Ok(format!(".{final_name}.tmp-{:016x}", u64::from_be_bytes(b)))
}

/// Tells if a name is a temporary file of 5.9: `.<name>.tmp-<16 hex>`.
pub(crate) fn is_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.') else { return false };
    let Some(pos) = rest.rfind(".tmp-") else { return false };
    let hex = &rest[pos + 5..];
    pos > 0 && hex.len() == 16 && hex.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_names() {
        let n = temp_name("a.json").unwrap();
        assert!(is_temp_name(&n), "{n}");
        assert!(!is_temp_name(".git"));
        assert!(!is_temp_name(".a.json.tmp-xyz"));
        assert!(!is_temp_name("a.json"));
    }

    #[test]
    fn write_and_read() {
        let t = tempfile::tempdir().unwrap();
        let root = Dir::open_root(t.path(), Faults::default()).unwrap();
        root.write_atomic("a.json", b"hello", None).unwrap();
        let (b, st) = root.read_file("a.json", 100).unwrap();
        assert_eq!(b, b"hello");
        assert_eq!(st.mode & !FILE_MODE, 0);
        let items = root.list().unwrap();
        assert_eq!(items.len(), 1);
        assert!(root.read_file("nope", 10).is_err());
    }
}
