use std::path::{Path, PathBuf};

/// A sensitive, local-only project locator. It deliberately implements
/// neither `Debug` nor serialization so it cannot accidentally enter reports,
/// logs, events, or persistence without passing through a project resolver.
pub struct ProjectLocator(PathBuf);

impl ProjectLocator {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}
