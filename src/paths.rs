use std::{env, path::PathBuf};

use anyhow::{Context, Result};

pub(crate) fn default_data_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/share"))
}
