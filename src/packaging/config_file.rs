use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use tempfile::NamedTempFile;

const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(50);

pub(super) fn read_config(path: &Path, provider: &str) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error)
            .with_context(|| format!("could not read {provider} config at {}", path.display())),
    }
}

pub(super) fn containing_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

pub(super) struct ConfigLock {
    file: File,
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(super) fn acquire_config_lock(
    directory: &Path,
    filename: &str,
    provider: &str,
) -> Result<ConfigLock> {
    let lock_path = directory.join(filename);
    if fs::symlink_metadata(&lock_path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("refusing to use symlinked {provider} config lock");
    }

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options
        .open(&lock_path)
        .with_context(|| format!("could not open {provider} config lock"))?;
    let started = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(ConfigLock { file }),
            Err(error)
                if error.kind() == ErrorKind::WouldBlock
                    && started.elapsed() < LOCK_WAIT_TIMEOUT =>
            {
                thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                bail!("timed out waiting for {provider} config lock");
            }
            Err(error) => {
                return Err(error).with_context(|| format!("could not lock {provider} config"));
            }
        }
    }
}

pub(super) fn atomic_write_config(config: &Path, content: &[u8], provider: &str) -> Result<()> {
    let write_path = resolved_write_path(config, provider)?;
    let directory = containing_directory(&write_path);
    let existing_permissions = fs::metadata(&write_path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut temporary = NamedTempFile::new_in(directory)
        .with_context(|| format!("could not create temporary {provider} config"))?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    let persisted = temporary
        .persist(&write_path)
        .map_err(|error| error.error)?;
    if let Some(permissions) = existing_permissions {
        persisted.set_permissions(permissions)?;
    }
    if let Ok(directory_file) = File::open(directory) {
        let _ = directory_file.sync_all();
    }
    Ok(())
}

fn resolved_write_path(config: &Path, provider: &str) -> Result<PathBuf> {
    match fs::symlink_metadata(config) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::canonicalize(config)
                .with_context(|| format!("could not resolve symlinked {provider} config"))?;
            if !fs::metadata(&target)?.is_file() {
                bail!("{provider} config symlink does not resolve to a regular file");
            }
            Ok(target)
        }
        Ok(metadata) if metadata.is_file() => Ok(config.to_path_buf()),
        Ok(_) => bail!("{provider} config path is not a regular file"),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(config.to_path_buf()),
        Err(error) => Err(error).with_context(|| format!("could not inspect {provider} config")),
    }
}
