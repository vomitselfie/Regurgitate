use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail};

const DATA_DIRECTORY: &str = "regurgitate";
const LEGACY_DATA_DIRECTORY: &str = "praxis";
const HISTORY_FILENAME: &str = "history.db";

pub(crate) fn history_database_for_read(data_home: &Path) -> PathBuf {
    let current = current_database(data_home);
    match fs::metadata(&current) {
        Ok(_) => current,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let legacy = legacy_database(data_home);
            if fs::metadata(&legacy).is_ok() {
                legacy
            } else {
                current
            }
        }
        Err(_) => current,
    }
}

pub(crate) fn prepare_history_database(data_home: &Path) -> Result<PathBuf> {
    let current_directory = data_home.join(DATA_DIRECTORY);
    match fs::symlink_metadata(&current_directory) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if regular_file_exists(&legacy_database(data_home), "legacy history database")? {
                migrate_legacy_directory(data_home)?;
            }
        }
        Err(error) => {
            return Err(error).context("could not inspect the Regurgitate data directory");
        }
    }
    prepare_private_directory(&current_directory)?;
    Ok(current_directory.join(HISTORY_FILENAME))
}

pub(crate) fn existing_history_database(
    data_home: &Path,
    writable: bool,
) -> Result<Option<PathBuf>> {
    let current = current_database(data_home);
    if regular_file_exists(&current, "Regurgitate history database")? {
        return Ok(Some(current));
    }

    let legacy = legacy_database(data_home);
    if !regular_file_exists(&legacy, "legacy history database")? {
        return Ok(None);
    }
    if !writable {
        return Ok(Some(legacy));
    }

    migrate_legacy_directory(data_home)?;
    Ok(Some(current))
}

fn current_database(data_home: &Path) -> PathBuf {
    data_home.join(DATA_DIRECTORY).join(HISTORY_FILENAME)
}

fn legacy_database(data_home: &Path) -> PathBuf {
    data_home.join(LEGACY_DATA_DIRECTORY).join(HISTORY_FILENAME)
}

fn regular_file_exists(path: &Path, description: &str) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => bail!("{description} is not a regular file"),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("could not inspect {description}")),
    }
}

fn migrate_legacy_directory(data_home: &Path) -> Result<()> {
    let legacy_directory = data_home.join(LEGACY_DATA_DIRECTORY);
    let current_directory = data_home.join(DATA_DIRECTORY);
    let metadata = match fs::symlink_metadata(&legacy_directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if regular_file_exists(&current_database(data_home), "Regurgitate history database")? {
                return Ok(());
            }
            return Err(error).context("could not inspect the legacy data directory");
        }
        Err(error) => return Err(error).context("could not inspect the legacy data directory"),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("legacy data directory is not a regular directory");
    }
    match fs::symlink_metadata(&current_directory) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Ok(_)
            if regular_file_exists(
                &current_database(data_home),
                "Regurgitate history database",
            )? =>
        {
            return Ok(());
        }
        Ok(_) => bail!(
            "cannot migrate legacy history because the Regurgitate data directory already exists"
        ),
        Err(error) => {
            return Err(error).context("could not inspect the Regurgitate data directory");
        }
    }
    if let Err(error) = fs::rename(&legacy_directory, &current_directory) {
        if regular_file_exists(&current_database(data_home), "Regurgitate history database")? {
            return Ok(());
        }
        return Err(error).context("could not migrate the legacy history directory to Regurgitate");
    }
    secure_directory(&current_directory)?;
    Ok(())
}

fn prepare_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| {
        format!(
            "could not create Regurgitate data directory at {}",
            path.display()
        )
    })?;
    secure_directory(path)
}

fn secure_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn read_path_falls_back_without_migrating_legacy_history() {
        let temp = tempdir().unwrap();
        let legacy = legacy_database(temp.path());
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"LEGACY_HISTORY").unwrap();

        assert_eq!(history_database_for_read(temp.path()), legacy);
        assert!(!temp.path().join(DATA_DIRECTORY).exists());
    }

    #[test]
    fn first_write_migrates_the_complete_legacy_directory() {
        let temp = tempdir().unwrap();
        let legacy = legacy_database(temp.path());
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"LEGACY_HISTORY").unwrap();
        fs::write(
            legacy.parent().unwrap().join("history.db-wal"),
            b"LEGACY_WAL",
        )
        .unwrap();

        let current = prepare_history_database(temp.path()).unwrap();

        assert_eq!(fs::read(&current).unwrap(), b"LEGACY_HISTORY");
        assert_eq!(
            fs::read(current.parent().unwrap().join("history.db-wal")).unwrap(),
            b"LEGACY_WAL"
        );
        assert!(!temp.path().join(LEGACY_DATA_DIRECTORY).exists());
    }

    #[test]
    fn new_history_uses_the_regurgitate_directory() {
        let temp = tempdir().unwrap();

        let database = prepare_history_database(temp.path()).unwrap();

        assert_eq!(database, temp.path().join("regurgitate/history.db"));
        assert!(database.parent().unwrap().is_dir());
    }
}
