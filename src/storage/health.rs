use std::{fs, io::ErrorKind, path::PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};

use crate::application::HistoryReadinessProbe;

/// Read-only inspection adapter for an existing Praxis history database.
pub struct HistoryDatabaseProbe {
    path: PathBuf,
}

impl HistoryDatabaseProbe {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl HistoryReadinessProbe for HistoryDatabaseProbe {
    fn event_count(&self) -> Result<Option<u64>> {
        match fs::metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => bail!("history database is not a regular file"),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("could not inspect history database"),
        }

        let connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("could not open history database read-only")?;
        let integrity: String =
            connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if integrity != "ok" {
            bail!("history database failed its integrity check");
        }
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        let count = count
            .try_into()
            .context("history database returned an invalid event count")?;
        Ok(Some(count))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::storage::{EncryptedStore, MasterKey};

    use super::*;

    #[test]
    fn missing_history_is_reported_without_creating_anything() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing/history.db");
        let probe = HistoryDatabaseProbe::new(path.clone());
        assert_eq!(probe.event_count().unwrap(), None);
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }

    #[test]
    fn existing_history_is_counted_read_only() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        let key = MasterKey::from_bytes([61; 32]);
        let store = EncryptedStore::open(&path, &key).unwrap();
        assert_eq!(store.count().unwrap(), 0);
        drop(store);

        let before = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            HistoryDatabaseProbe::new(path.clone())
                .event_count()
                .unwrap(),
            Some(0)
        );
        let after = fs::metadata(path).unwrap().modified().unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn damaged_history_is_unavailable_instead_of_being_repaired() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        fs::write(&path, b"PRIVATE_CORRUPTED_DATABASE").unwrap();

        assert!(
            HistoryDatabaseProbe::new(path.clone())
                .event_count()
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), b"PRIVATE_CORRUPTED_DATABASE");
    }
}
