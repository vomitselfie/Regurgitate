use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail};

const DATA_DIRECTORY: &str = "regurgitate";
const HISTORY_FILENAME: &str = "history.db";

pub(crate) fn history_database_for_read(data_home: &Path) -> PathBuf {
    current_database(data_home)
}

pub(crate) fn prepare_history_database(data_home: &Path) -> Result<PathBuf> {
    let current_directory = data_home.join(DATA_DIRECTORY);
    prepare_private_directory(&current_directory)?;
    Ok(current_directory.join(HISTORY_FILENAME))
}

pub(crate) fn existing_history_database(data_home: &Path) -> Result<Option<PathBuf>> {
    let current = current_database(data_home);
    if regular_file_exists(&current, "Regurgitate history database")? {
        Ok(Some(current))
    } else {
        Ok(None)
    }
}

fn current_database(data_home: &Path) -> PathBuf {
    data_home.join(DATA_DIRECTORY).join(HISTORY_FILENAME)
}

fn regular_file_exists(path: &Path, description: &str) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => bail!("{description} is not a regular file"),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("could not inspect {description}")),
    }
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
    fn read_path_does_not_create_state() {
        let temp = tempdir().unwrap();
        let expected = temp.path().join("regurgitate/history.db");

        assert_eq!(history_database_for_read(temp.path()), expected);
        assert!(!temp.path().join(DATA_DIRECTORY).exists());
    }

    #[test]
    fn new_history_uses_the_regurgitate_directory() {
        let temp = tempdir().unwrap();

        let database = prepare_history_database(temp.path()).unwrap();

        assert_eq!(database, temp.path().join("regurgitate/history.db"));
        assert!(database.parent().unwrap().is_dir());
    }
}
