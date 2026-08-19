use std::{env, fs, io, path::PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result};
use zeroize::Zeroizing;

use crate::{
    adapters::{ManagedCodexSource, aoe, aoe_hook, codex},
    application::ProjectLocator,
    application::{IngestionReport, IngestionService, SessionEventSource},
    cli::{Cli, Command},
    core::{AgentKind, DebugEvent},
    packaging::install_skill,
    query::{RecallOptions, RecallResult, RecallService},
    storage::{EncryptedStore, MasterKeyProvider, SecretServiceKeyProvider},
};

pub fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Command::DebugHook => {
            let event = codex::normalize_post_tool_hook(io::stdin().lock())?;
            print_json(&DebugEvent::from(&event))
        }
        Command::AoeHook => {
            let context = aoe_hook::current_context()?;
            let report = ingest_aoe_hook(
                context,
                None,
                None,
                default_data_home()?,
                &SecretServiceKeyProvider::default(),
            )?;
            print_json(&report)
        }
        Command::PrintAoeConfig => {
            println!("{AOE_CONFIG_SNIPPET}");
            Ok(())
        }
        Command::InstallSkill { target, apply } => {
            let report = install_skill(&target, apply)?;
            print_json(&report)
        }
        Command::DebugParse {
            session,
            profile,
            aoe_config_dir,
            codex_home,
        } => {
            let source = managed_codex_source(profile, aoe_config_dir, codex_home)?;
            let batch = source.events_for_session(&session, None)?;
            let output: Vec<_> = batch.events.iter().map(DebugEvent::from).collect();
            print_json(&output)
        }
        Command::Ingest {
            session,
            profile,
            aoe_config_dir,
            codex_home,
            data_home,
        } => {
            let source = managed_codex_source(profile, aoe_config_dir, codex_home)?;
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = ingest_session(
                source,
                &session,
                data_home,
                &SecretServiceKeyProvider::default(),
            )?;
            print_json(&report)
        }
        Command::Recall {
            project,
            operation,
            failures,
            limit,
            query,
            token_budget,
            data_home,
        } => {
            let query = query.map(Zeroizing::new);
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let result = recall_project(
                project,
                RecallOptions {
                    operation: operation.map(Into::into),
                    failures_only: failures,
                    limit,
                    token_budget,
                },
                query.as_ref().map(|value| value.as_str()),
                data_home,
                &SecretServiceKeyProvider::default(),
            )?;
            print_json(&result)
        }
    }
}

const AOE_CONFIG_SNIPPET: &str = r#"# Merge into the global or profile AoE config.
# The handler reads only AOE_SESSION_ID, AOE_PROFILE, and AOE_TOOL.
[status_hooks]
enabled = true
on_idle = "praxis aoe-hook"
on_error = "praxis aoe-hook""#;

#[derive(serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum AoeHookReport {
    Ingested {
        #[serde(flatten)]
        report: IngestionReport,
    },
    IgnoredUnsupportedAgent,
}

fn ingest_aoe_hook(
    context: aoe_hook::AoeHookContext,
    aoe_config_dir: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<AoeHookReport> {
    if context.agent != AgentKind::Codex {
        return Ok(AoeHookReport::IgnoredUnsupportedAgent);
    }
    let source = managed_codex_source(context.profile, aoe_config_dir, codex_home)?;
    let report = ingest_session(source, &context.session_id, data_home, key_provider)?;
    Ok(AoeHookReport::Ingested { report })
}

fn managed_codex_source(
    profile: String,
    aoe_config_dir: Option<PathBuf>,
    codex_home: Option<PathBuf>,
) -> Result<ManagedCodexSource> {
    let config_dir = aoe_config_dir
        .map(Ok)
        .unwrap_or_else(aoe::default_config_dir)?;
    let codex_home = codex_home.map(Ok).unwrap_or_else(aoe::default_codex_home)?;
    Ok(ManagedCodexSource::new(config_dir, profile, codex_home))
}

fn ingest_session(
    source: ManagedCodexSource,
    session_id: &str,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<IngestionReport> {
    let praxis_dir = data_home.join("praxis");
    prepare_private_directory(&praxis_dir)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&praxis_dir.join("history.db"), &key)?;
    IngestionService::new(source, store).ingest_session(session_id)
}

fn recall_project(
    project: PathBuf,
    options: RecallOptions,
    task_query: Option<&str>,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<RecallResult> {
    let praxis_dir = data_home.join("praxis");
    prepare_private_directory(&praxis_dir)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&praxis_dir.join("history.db"), &key)?;
    RecallService::new(&store).recall(&ProjectLocator::new(project), options, task_query)
}

fn default_data_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/share"))
}

fn prepare_private_directory(path: &std::path::Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| {
        format!(
            "could not create Praxis data directory at {}",
            path.display()
        )
    })?;

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer_pretty(io::stdout().lock(), value)?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
    };

    use tempfile::tempdir;

    use crate::storage::MasterKey;

    use super::*;

    struct FixedKeyProvider;

    impl MasterKeyProvider for FixedKeyProvider {
        fn get_or_create(&self) -> Result<MasterKey> {
            Ok(MasterKey::from_bytes([23; 32]))
        }
    }

    #[test]
    fn ingestion_is_encrypted_and_idempotent_without_a_live_keyring() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("aoe");
        let profile_dir = config_dir.join("profiles/default");
        fs::create_dir_all(&profile_dir).unwrap();
        let project_dir = temp.path().join("PLAINTEXT_SENTINEL_PROJECT");
        fs::create_dir(&project_dir).unwrap();
        fs::write(
            profile_dir.join("sessions.json"),
            serde_json::to_vec(&serde_json::json!([{
                "id": "aoe-1",
                "project_path": project_dir,
                "tool": "codex",
                "agent_session_id": "codex-1"
            }]))
            .unwrap(),
        )
        .unwrap();

        let codex_home = temp.path().join("codex");
        let transcript_dir = codex_home.join("sessions/2026/08/19");
        fs::create_dir_all(&transcript_dir).unwrap();
        let transcript_path = transcript_dir.join("rollout-codex-1.jsonl");
        fs::write(
            &transcript_path,
            concat!(
                "{\"timestamp\":\"2026-08-19T12:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"call-1\",\"arguments\":\"SECRET_ARGUMENT\"}}\n",
                "{\"timestamp\":\"2026-08-19T12:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"call-1\",\"output\":{\"exit_code\":0,\"output\":\"SECRET_OUTPUT\"}}}\n"
            ),
        )
        .unwrap();

        let source = || ManagedCodexSource::new(config_dir.clone(), "default", codex_home.clone());
        let data_home = temp.path().join("data");
        let first =
            ingest_session(source(), "aoe-1", data_home.clone(), &FixedKeyProvider).unwrap();
        let second =
            ingest_session(source(), "aoe-1", data_home.clone(), &FixedKeyProvider).unwrap();

        assert_eq!(first.observed, 1);
        assert_eq!(first.inserted, 1);
        assert_eq!(second.observed, 0);
        assert_eq!(second.inserted, 0);

        let hook_report = ingest_aoe_hook(
            aoe_hook::AoeHookContext {
                session_id: "aoe-1".to_owned(),
                profile: "default".to_owned(),
                agent: AgentKind::Codex,
            },
            Some(config_dir.clone()),
            Some(codex_home.clone()),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        let AoeHookReport::Ingested { report } = hook_report else {
            panic!("Codex hook was unexpectedly ignored");
        };
        assert_eq!(report.observed, 0);
        assert_eq!(report.inserted, 0);

        let mut transcript = OpenOptions::new()
            .append(true)
            .open(&transcript_path)
            .unwrap();
        transcript
            .write_all(
                concat!(
                    "{\"timestamp\":\"2026-08-19T12:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"apply_patch\",\"call_id\":\"call-2\",\"arguments\":\"SECOND_SECRET_ARGUMENT\"}}\n",
                    "{\"timestamp\":\"2026-08-19T12:00:03Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"call-2\",\"output\":{\"success\":true,\"output\":\"SECOND_SECRET_OUTPUT\"}}}\n"
                )
                .as_bytes(),
            )
            .unwrap();
        drop(transcript);
        let third =
            ingest_session(source(), "aoe-1", data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(third.observed, 1);
        assert_eq!(third.inserted, 1);

        let recalled = recall_project(
            project_dir,
            RecallOptions::default(),
            Some("SUPER_SECRET_QUERY"),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(recalled.observations.len(), 2);
        let recalled_json = serde_json::to_string(&recalled).unwrap();
        for forbidden in ["aoe-1", "PLAINTEXT_SENTINEL_PROJECT", "SECRET"] {
            assert!(!recalled_json.contains(forbidden));
        }

        let database = fs::read(data_home.join("praxis/history.db")).unwrap();
        for forbidden in [
            b"PLAINTEXT_SENTINEL_PROJECT".as_slice(),
            b"SECRET_ARGUMENT",
            b"SECRET_OUTPUT",
            b"SECOND_SECRET_ARGUMENT",
            b"SECOND_SECRET_OUTPUT",
            b"SUPER_SECRET_QUERY",
            b"aoe-1",
        ] {
            assert!(
                !database
                    .windows(forbidden.len())
                    .any(|window| window == forbidden)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn runtime_data_directory_is_owner_only() {
        let temp = tempdir().unwrap();
        let praxis_dir = temp.path().join("data/praxis");
        prepare_private_directory(&praxis_dir).unwrap();
        let mode = fs::metadata(praxis_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn generated_aoe_config_uses_the_identifier_only_handler() {
        assert!(AOE_CONFIG_SNIPPET.contains("on_idle = \"praxis aoe-hook\""));
        assert!(AOE_CONFIG_SNIPPET.contains("on_error = \"praxis aoe-hook\""));
        for forbidden in [
            "AOE_PROJECT_PATH",
            "AOE_SESSION_TITLE",
            "AOE_GROUP_PATH",
            "AOE_NEW_STATUS",
        ] {
            assert!(!AOE_CONFIG_SNIPPET.contains(forbidden));
        }
    }
}
