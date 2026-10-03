use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn digest(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        text.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        text.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    text
}

pub(crate) fn unique_id() -> String {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{clock}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn read_regular(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK | nix::libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .context("opening assigned regular file")?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum {
        bail!("assigned file is not regular or exceeds byte budget");
    }
    let mut bytes = Vec::new();
    file.take(maximum.checked_add(1).context("file read limit overflow")?)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        bail!("assigned file grew beyond byte budget");
    }
    Ok(bytes)
}

pub(crate) fn encode<T: Serialize + ?Sized>(value: &T, maximum: u64) -> Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        maximum: u64,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let length = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("encoded byte count overflow"))?;
            if length as u64 > self.maximum {
                return Err(std::io::Error::other("encoded JSON exceeds byte budget"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: vec![],
        maximum,
    };
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.bytes)
}

pub(crate) fn encoded_size<T: Serialize + ?Sized>(value: &T, maximum: u64) -> Result<u64> {
    struct Count {
        bytes: u64,
        maximum: u64,
    }
    impl Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let size = self
                .bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| std::io::Error::other("encoded size overflow"))?;
            if size > self.maximum {
                return Err(std::io::Error::other("encoded evidence exceeds budget"));
            }
            self.bytes = size;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { bytes: 0, maximum };
    serde_json::to_writer(&mut count, value)?;
    Ok(count.bytes)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("assigned file has no parent")?;
    let temporary = parent.join(format!(".archguard-{}.tmp", unique_id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(crate) fn create_private_directory(parent: &Path, prefix: &str) -> Result<PathBuf> {
    let path = parent.join(format!("{prefix}-{}", unique_id()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_regular_io_and_atomic_replacement() {
        let directory =
            create_private_directory(&std::env::temp_dir(), "archguard-storage-test").unwrap();
        let file = directory.join("record.json");
        atomic_write(&file, b"old").unwrap();
        atomic_write(&file, b"new").unwrap();
        assert_eq!(read_regular(&file, 3).unwrap(), b"new");
        assert!(read_regular(&file, 2).is_err());
        assert!(read_regular(&directory, 100).is_err());
        std::os::unix::fs::symlink(&file, directory.join("alias")).unwrap();
        assert!(read_regular(&directory.join("alias"), 100).is_err());
        assert!(encode(&vec!["x"; 100], 10).is_err());
        assert!(encode(&vec!["x"; 1], 10).is_ok());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        fs::remove_dir_all(directory).unwrap();
    }
}
