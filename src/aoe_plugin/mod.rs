mod protocol;
mod setup;
mod view;

use std::{env, ffi::OsStr, io};

use anyhow::Result;

use crate::{
    application::{HealthReport, HealthService},
    storage::{HistoryDatabaseProbe, SystemKeyProvider, history_database_for_read},
};

pub const PLUGIN_ID: &str = "vomitselfie.regurgitate";

pub fn is_worker_invocation() -> bool {
    matches_worker_invocation(
        env::var_os("AOE_PLUGIN_ID").as_deref(),
        env::args_os().count(),
    )
}

pub fn run() -> Result<()> {
    let data_home = crate::paths::default_data_home()?;
    let setup = setup::SetupService::from_environment()?;
    let inspect = || PluginSnapshot {
        health: inspect_health(&data_home),
        integrations: setup.inspect(),
    };
    let configure = |target| setup.setup(target);
    protocol::run(
        io::BufReader::new(io::stdin()),
        io::stdout().lock(),
        inspect,
        configure,
    )
}

pub(super) struct PluginSnapshot {
    health: HealthReport,
    integrations: setup::IntegrationOverview,
}

fn matches_worker_invocation(plugin_id: Option<&OsStr>, argument_count: usize) -> bool {
    plugin_id == Some(OsStr::new(PLUGIN_ID)) && argument_count == 1
}

fn inspect_health(data_home: &std::path::Path) -> HealthReport {
    let history = HistoryDatabaseProbe::new(history_database_for_read(data_home));
    HealthService::new(SystemKeyProvider::default(), history).inspect()
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
            Some("regurgitate-v${version}-${os}-${arch}.tar.gz")
        );
        assert_eq!(manifest["runtime"]["bin"].as_str(), Some("regurgitate"));

        let capabilities: Vec<_> = manifest["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|value| value.as_str())
            .collect();
        assert_eq!(capabilities, ["runtime.worker", "fs.read", "fs.write"]);

        let command_ids: Vec<_> = manifest["commands"]
            .as_array_of_tables()
            .unwrap()
            .iter()
            .filter_map(|command| command["id"].as_str())
            .collect();
        assert_eq!(
            command_ids,
            ["status", "refresh", "setup-codex", "setup-claude"]
        );
    }
}
