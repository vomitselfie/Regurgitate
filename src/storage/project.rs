use std::{fs, path::Path};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::{ProjectLocator, ProjectResolver};
use crate::query::ProjectLookup;

use super::{
    private::{PRIVATE_ENVELOPE_VERSION, PrivateRecordKind},
    sqlite::EncryptedStore,
};

const PROJECT_RECORD_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRecord {
    project_id: Uuid,
    canonical_path: Vec<u8>,
    schema_version: u32,
}

struct StoredPrivateRecord {
    envelope_version: i64,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

pub(super) struct ProjectIdentity {
    pub lookup_token: [u8; 32],
    pub project_id: Uuid,
}

impl ProjectResolver for EncryptedStore {
    fn resolve_project(&self, locator: &ProjectLocator) -> Result<Uuid> {
        let canonical_path = canonical_path_bytes(locator.as_path())?;
        let lookup_token = self
            .private_cipher
            .lookup_token(PrivateRecordKind::Project, &canonical_path)?;

        if let Some(stored) = self.load_project_record(&lookup_token)? {
            return self.open_project_record(&lookup_token, stored, &canonical_path);
        }

        let project = ProjectRecord {
            project_id: Uuid::new_v4(),
            canonical_path: canonical_path.clone(),
            schema_version: PROJECT_RECORD_VERSION,
        };
        let sealed =
            self.private_cipher
                .seal(PrivateRecordKind::Project, &lookup_token, &project)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO private_projects
                (lookup_token, envelope_version, nonce, ciphertext)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                lookup_token.as_slice(),
                i64::from(PRIVATE_ENVELOPE_VERSION),
                sealed.nonce.as_slice(),
                sealed.ciphertext,
            ],
        )?;

        let stored = self
            .load_project_record(&lookup_token)?
            .context("project identity disappeared during creation")?;
        self.open_project_record(&lookup_token, stored, &canonical_path)
    }
}

impl ProjectLookup for EncryptedStore {
    fn find_project(&self, locator: &ProjectLocator) -> Result<Option<Uuid>> {
        Ok(self
            .find_project_identity_in(&self.connection, locator)?
            .map(|identity| identity.project_id))
    }
}

impl EncryptedStore {
    pub(super) fn find_project_identity_in(
        &self,
        connection: &Connection,
        locator: &ProjectLocator,
    ) -> Result<Option<ProjectIdentity>> {
        let canonical_path = canonical_path_bytes(locator.as_path())?;
        let lookup_token = self
            .private_cipher
            .lookup_token(PrivateRecordKind::Project, &canonical_path)?;
        let Some(stored) = Self::load_project_record_from(connection, &lookup_token)? else {
            return Ok(None);
        };
        let project_id = self.open_project_record(&lookup_token, stored, &canonical_path)?;
        Ok(Some(ProjectIdentity {
            lookup_token,
            project_id,
        }))
    }

    fn load_project_record(&self, lookup_token: &[u8; 32]) -> Result<Option<StoredPrivateRecord>> {
        Self::load_project_record_from(&self.connection, lookup_token)
    }

    fn load_project_record_from(
        connection: &Connection,
        lookup_token: &[u8; 32],
    ) -> Result<Option<StoredPrivateRecord>> {
        connection
            .query_row(
                "SELECT envelope_version, nonce, ciphertext
                 FROM private_projects WHERE lookup_token = ?1",
                [lookup_token.as_slice()],
                |row| {
                    Ok(StoredPrivateRecord {
                        envelope_version: row.get(0)?,
                        nonce: row.get(1)?,
                        ciphertext: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn open_project_record(
        &self,
        lookup_token: &[u8; 32],
        stored: StoredPrivateRecord,
        expected_path: &[u8],
    ) -> Result<Uuid> {
        let envelope_version = stored
            .envelope_version
            .try_into()
            .context("invalid project envelope version in database")?;
        let record: ProjectRecord = self.private_cipher.open(
            PrivateRecordKind::Project,
            lookup_token,
            envelope_version,
            &stored.nonce,
            &stored.ciphertext,
        )?;
        if record.schema_version != PROJECT_RECORD_VERSION || record.canonical_path != expected_path
        {
            bail!("encrypted project mapping does not match its lookup identity");
        }
        Ok(record.project_id)
    }
}

fn canonical_path_bytes(path: &Path) -> Result<Vec<u8>> {
    let canonical = fs::canonicalize(path).context("could not resolve the local project path")?;
    let canonical = repository_root(&canonical).unwrap_or(canonical);
    #[cfg(unix)]
    {
        Ok(canonical.as_os_str().as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        Ok(canonical.to_string_lossy().as_bytes().to_vec())
    }
}

/// Walk-up bound; deeper nesting falls back to the directory itself.
const MAX_ANCESTORS: usize = 64;

/// Project identity is the repository, not the directory an agent happens to
/// run from. A subdirectory resolves to its checkout root; a linked git
/// worktree resolves to the main checkout it was created from. A path with
/// no enclosing `.git` keeps its own identity, so scratch directories stay
/// isolated. Nothing here shells out or reads repository content.
fn repository_root(canonical: &Path) -> Option<std::path::PathBuf> {
    let mut current = Some(canonical);
    for _ in 0..MAX_ANCESTORS {
        let directory = current?;
        let marker = directory.join(".git");
        let Ok(metadata) = fs::symlink_metadata(&marker) else {
            current = directory.parent();
            continue;
        };
        if metadata.is_dir() {
            return Some(directory.to_path_buf());
        }
        if metadata.is_file() {
            return Some(
                main_checkout_from_gitdir_file(&marker).unwrap_or_else(|| directory.to_path_buf()),
            );
        }
        current = directory.parent();
    }
    None
}

/// A linked worktree's `.git` file reads `gitdir: <main>/.git/worktrees/<name>`.
/// Resolves `<main>` when the file has that exact shape; anything else keeps
/// the worktree's own identity.
fn main_checkout_from_gitdir_file(marker: &Path) -> Option<std::path::PathBuf> {
    let content = fs::read_to_string(marker).ok()?;
    if content.len() > 4_096 {
        return None;
    }
    let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
    let gitdir = Path::new(gitdir);
    let gitdir = if gitdir.is_absolute() {
        gitdir.to_path_buf()
    } else {
        marker.parent()?.join(gitdir)
    };
    let gitdir = fs::canonicalize(gitdir).ok()?;
    // <main>/.git/worktrees/<name>
    let worktrees = gitdir.parent()?;
    if worktrees.file_name()?.to_str()? != "worktrees" {
        return None;
    }
    let dot_git = worktrees.parent()?;
    if dot_git.file_name()?.to_str()? != ".git" {
        return None;
    }
    let main = dot_git.parent()?;
    fs::metadata(main)
        .ok()?
        .is_dir()
        .then(|| main.to_path_buf())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::storage::MasterKey;

    use super::*;

    #[test]
    fn subdirectories_and_worktrees_resolve_to_the_main_checkout() {
        let temp = tempdir().unwrap();
        let main = temp.path().join("repo");
        fs::create_dir_all(main.join(".git/worktrees/feature")).unwrap();
        fs::create_dir_all(main.join("crates/inner/src")).unwrap();
        let worktree = temp.path().join("repo-feature");
        fs::create_dir_all(worktree.join("src")).unwrap();
        fs::write(
            worktree.join(".git"),
            format!(
                "gitdir: {}\n",
                main.join(".git/worktrees/feature").display()
            ),
        )
        .unwrap();
        let scratch = temp.path().join("scratch");
        fs::create_dir(&scratch).unwrap();

        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([32; 32])).unwrap();
        let root = store
            .resolve_project(&ProjectLocator::new(main.clone()))
            .unwrap();
        for path in [
            main.join("crates/inner/src"),
            worktree.clone(),
            worktree.join("src"),
        ] {
            assert_eq!(
                store
                    .resolve_project(&ProjectLocator::new(path.clone()))
                    .unwrap(),
                root,
                "{} should share the checkout identity",
                path.display()
            );
        }
        let isolated = store
            .resolve_project(&ProjectLocator::new(scratch))
            .unwrap();
        assert_ne!(isolated, root);
        let mappings: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM private_projects", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(mappings, 2);
    }

    #[test]
    fn malformed_gitdir_files_keep_the_directory_identity() {
        let temp = tempdir().unwrap();
        let odd = temp.path().join("odd");
        fs::create_dir(&odd).unwrap();
        fs::write(odd.join(".git"), "gitdir: /nonexistent/elsewhere\n").unwrap();
        assert_eq!(
            repository_root(&fs::canonicalize(&odd).unwrap()),
            Some(fs::canonicalize(&odd).unwrap())
        );
        let submodule_like = temp.path().join("sub");
        fs::create_dir_all(temp.path().join("parent/.git/modules/sub")).unwrap();
        fs::create_dir(&submodule_like).unwrap();
        fs::write(
            submodule_like.join(".git"),
            format!(
                "gitdir: {}\n",
                temp.path().join("parent/.git/modules/sub").display()
            ),
        )
        .unwrap();
        assert_eq!(
            repository_root(&fs::canonicalize(&submodule_like).unwrap()),
            Some(fs::canonicalize(&submodule_like).unwrap())
        );
    }

    #[test]
    fn stable_project_id_uses_no_plaintext_path() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("PLAINTEXT_SENTINEL_PROJECT");
        fs::create_dir(&project).unwrap();
        let database = temp.path().join("history.db");
        let id = {
            let store = EncryptedStore::open(&database, &MasterKey::from_bytes([31; 32])).unwrap();
            let locator = ProjectLocator::new(project.clone());
            assert_eq!(store.find_project(&locator).unwrap(), None);
            let before: i64 = store
                .connection
                .query_row("SELECT COUNT(*) FROM private_projects", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(before, 0, "lookup unexpectedly created a project mapping");
            let first = store.resolve_project(&locator).unwrap();
            let second = store.resolve_project(&locator).unwrap();
            assert_eq!(first, second);
            first
        };
        let reopened = EncryptedStore::open(&database, &MasterKey::from_bytes([31; 32])).unwrap();
        assert_eq!(
            reopened
                .resolve_project(&ProjectLocator::new(project.clone()))
                .unwrap(),
            id
        );
        drop(reopened);
        assert_ne!(id, Uuid::nil());
        let bytes = fs::read(database).unwrap();
        assert!(
            !bytes
                .windows(b"PLAINTEXT_SENTINEL_PROJECT".len())
                .any(|window| window == b"PLAINTEXT_SENTINEL_PROJECT")
        );
    }
}
