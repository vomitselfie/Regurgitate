mod protocol;
mod view;

use std::{env, ffi::OsStr, io};

use anyhow::Result;

use crate::{
    application::{HealthReport, HealthService},
    storage::{HistoryDatabaseProbe, SecretServiceKeyProvider},
};

pub const PLUGIN_ID: &str = "vomitselfie.praxis";

pub fn is_worker_invocation() -> bool {
    matches_worker_invocation(
        env::var_os("AOE_PLUGIN_ID").as_deref(),
        env::args_os().count(),
    )
}

pub fn run() -> Result<()> {
    let data_home = crate::paths::default_data_home()?;
    let inspect = move || inspect_health(&data_home);
    protocol::run(
        io::BufReader::new(io::stdin()),
        io::stdout().lock(),
        inspect,
    )
}

fn matches_worker_invocation(plugin_id: Option<&OsStr>, argument_count: usize) -> bool {
    plugin_id == Some(OsStr::new(PLUGIN_ID)) && argument_count == 1
}

fn inspect_health(data_home: &std::path::Path) -> HealthReport {
    let history = HistoryDatabaseProbe::new(data_home.join("praxis/history.db"));
    HealthService::new(SecretServiceKeyProvider::default(), history).inspect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = include_str!("../../aoe-plugin.toml");

    #[test]
    fn worker_mode_requires_the_exact_aoe_identity_and_no_cli_arguments() {
        assert!(matches_worker_invocation(Some(OsStr::new(PLUGIN_ID)), 1));
        assert!(!matches_worker_invocation(Some(OsStr::new(PLUGIN_ID)), 2));
        assert!(!matches_worker_invocation(
            Some(OsStr::new("other.plugin")),
            1
        ));
        assert!(!matches_worker_invocation(None, 1));
    }

    #[test]
    fn manifest_matches_the_release_binary_layout() {
        let manifest = MANIFEST.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(manifest["id"].as_str(), Some(PLUGIN_ID));
        assert_eq!(
            manifest["version"].as_str(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(manifest["api_version"].as_integer(), Some(12));
        assert_eq!(manifest["runtime"]["kind"].as_str(), Some("release-binary"));
        assert_eq!(
            manifest["runtime"]["asset"].as_str(),
            Some("praxis-v${version}-x86_64-unknown-linux-musl.tar.gz")
        );
        let expected_binary = format!(
            "praxis-v{}-x86_64-unknown-linux-musl/praxis",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(
            manifest["runtime"]["bin"].as_str(),
            Some(expected_binary.as_str())
        );
    }
}
