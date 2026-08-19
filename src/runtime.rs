use std::{fs, io, path::PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result};
use zeroize::Zeroizing;

use crate::{
    adapters::{ManagedCodexSource, aoe, aoe_hook, claude, codex},
    application::{
        ForgetReport, ForgetService, ForgetStatus, HealthReport, HealthService, HookObservation,
        HookProvider, HookReadiness, IngestionReport, IngestionService, LearningReport,
        LearningService, ProjectLocator, RecordingReport, RecordingService, RetentionPolicy,
        RetentionReport, RetentionService, RetentionStatus, SessionEventSource,
        ValidatedRetentionPolicy,
    },
    cli::{Cli, Command, HookAgentArg},
    core::{AgentKind, DebugEvent, Outcome, Strategy},
    packaging::{
        AOE_CONFIG_SNIPPET, CLAUDE_CONFIG_SNIPPET, CODEX_CONFIG_SNIPPET, inspect_aoe_hook,
        inspect_claude_hook, inspect_codex_hook, install_aoe_hook, install_claude_hook,
        install_codex_hook, install_skill,
    },
    paths::default_data_home,
    query::{RecallOptions, RecallResult, RecallService},
    storage::{
        EncryptedStore, ExistingMasterKeyProvider, HistoryDatabaseProbe, MasterKeyProvider,
        SystemKeyProvider,
    },
};

pub fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Command::DebugHook { agent } => {
            let observation = normalize_hook(agent, io::stdin().lock())?;
            print_json(&DebugEvent::from(observation.event()))
        }
        Command::RecordHook { agent, data_home } => {
            let observation = normalize_hook(agent, io::stdin().lock())?;
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            record_hook(observation, data_home, &SystemKeyProvider::default())?;
            Ok(())
        }
        Command::Learn {
            project,
            strategy,
            outcome,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = learn_practice(
                project,
                strategy.into(),
                outcome.into(),
                data_home,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
        Command::AoeHook => {
            let context = aoe_hook::current_context()?;
            let report = ingest_aoe_hook(
                context,
                None,
                None,
                default_data_home()?,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
        Command::PrintAoeConfig => {
            println!("{AOE_CONFIG_SNIPPET}");
            Ok(())
        }
        Command::PrintCodexConfig => {
            println!("{CODEX_CONFIG_SNIPPET}");
            Ok(())
        }
        Command::PrintClaudeConfig => {
            println!("{CLAUDE_CONFIG_SNIPPET}");
            Ok(())
        }
        Command::Status {
            aoe_config,
            claude_config,
            codex_config,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let mut hooks = Vec::with_capacity(3);
            if let Some(config) = aoe_config {
                hooks.push((HookProvider::Aoe, inspect_aoe_hook(&config)));
            }
            if let Some(config) = claude_config {
                hooks.push((HookProvider::Claude, inspect_claude_hook(&config)));
            }
            if let Some(config) = codex_config {
                hooks.push((HookProvider::Codex, inspect_codex_hook(&config)));
            }
            let report = health_status(data_home, SystemKeyProvider::default(), hooks);
            print_json(&report)
        }
        Command::Forget {
            project,
            apply,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = forget_project(project, apply, data_home, &SystemKeyProvider::default())?;
            print_json(&report)
        }
        Command::Prune {
            older_than_days,
            keep_recent,
            apply,
            data_home,
        } => {
            let policy =
                retention_policy(older_than_days, keep_recent)?.validate(chrono::Utc::now())?;
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = prune_history(policy, apply, data_home, &SystemKeyProvider::default())?;
            print_json(&report)
        }
        Command::InstallAoeHook { config, apply } => {
            let report = install_aoe_hook(&config, apply)?;
            print_json(&report)
        }
        Command::InstallCodexHook { config, apply } => {
            let report = install_codex_hook(&config, apply)?;
            print_json(&report)
        }
        Command::InstallClaudeHook { config, apply } => {
            let report = install_claude_hook(&config, apply)?;
            print_json(&report)
        }
        Command::InstallSkill {
            target,
            apply,
            replace,
        } => {
            let report = install_skill(&target, apply, replace)?;
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
            let report =
                ingest_session(source, &session, data_home, &SystemKeyProvider::default())?;
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
                &SystemKeyProvider::default(),
            )?;
            print_json(&result)
        }
    }
}

fn normalize_hook(agent: HookAgentArg, reader: impl io::Read) -> Result<HookObservation> {
    match agent {
        HookAgentArg::Codex => codex::normalize_post_tool_observation(reader),
        HookAgentArg::Claude => claude::normalize_tool_hook(reader),
    }
}

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

fn record_hook(
    observation: HookObservation,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<RecordingReport> {
    let praxis_dir = data_home.join("praxis");
    prepare_private_directory(&praxis_dir)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&praxis_dir.join("history.db"), &key)?;
    RecordingService::new(store).record(observation)
}

fn learn_practice(
    project: PathBuf,
    strategy: Strategy,
    outcome: Outcome,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<LearningReport> {
    let praxis_dir = data_home.join("praxis");
    prepare_private_directory(&praxis_dir)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&praxis_dir.join("history.db"), &key)?;
    LearningService::new(store).learn(ProjectLocator::new(project), strategy, outcome)
}

fn health_status(
    data_home: PathBuf,
    key_probe: impl crate::application::KeyReadinessProbe,
    hooks: impl IntoIterator<Item = (HookProvider, Result<HookReadiness>)>,
) -> HealthReport {
    let history = HistoryDatabaseProbe::new(data_home.join("praxis/history.db"));
    HealthService::new(key_probe, history).inspect_with_hooks(hooks)
}

fn forget_project(
    project: PathBuf,
    apply: bool,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<ForgetReport> {
    let Some(store) = open_existing_history(&data_home, apply, key_provider)? else {
        return Ok(ForgetReport {
            status: ForgetStatus::NotFound,
            events: 0,
        });
    };
    ForgetService::new(store).forget(&ProjectLocator::new(project), apply)
}

fn retention_policy(
    older_than_days: Option<u32>,
    keep_recent: Option<u64>,
) -> Result<RetentionPolicy> {
    match (older_than_days, keep_recent) {
        (Some(days), None) => Ok(RetentionPolicy::OlderThanDays(days)),
        (None, Some(count)) => Ok(RetentionPolicy::KeepRecent(count)),
        _ => anyhow::bail!("exactly one retention policy is required"),
    }
}

fn prune_history(
    policy: ValidatedRetentionPolicy,
    apply: bool,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<RetentionReport> {
    let Some(store) = open_existing_history(&data_home, apply, key_provider)? else {
        return Ok(RetentionReport {
            status: RetentionStatus::NoChanges,
            events: 0,
        });
    };
    RetentionService::new(store).enforce(policy, apply)
}

fn open_existing_history(
    data_home: &std::path::Path,
    writable: bool,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<Option<EncryptedStore>> {
    let database = data_home.join("praxis/history.db");
    match fs::metadata(&database) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => anyhow::bail!("Praxis history database is not a regular file"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("could not inspect Praxis history database"),
    }
    let key = key_provider
        .get_existing()?
        .context("Praxis history exists but its master key is unavailable")?;
    let store = if writable {
        EncryptedStore::open_existing(&database, &key)?
    } else {
        EncryptedStore::open_read_only(&database, &key)?
    };
    Ok(Some(store))
}

fn recall_project(
    project: PathBuf,
    options: RecallOptions,
    task_query: Option<&str>,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<RecallResult> {
    options.validate()?;
    let Some(store) = open_existing_history(&data_home, false, key_provider)? else {
        return Ok(RecallResult::empty());
    };
    RecallService::new(&store).recall(&ProjectLocator::new(project), options, task_query)
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

    use crate::{
        application::KeyReadinessProbe,
        storage::{ExistingMasterKeyProvider, MasterKey},
    };

    use super::*;

    struct FixedKeyProvider;

    impl MasterKeyProvider for FixedKeyProvider {
        fn get_or_create(&self) -> Result<MasterKey> {
            Ok(MasterKey::from_bytes([23; 32]))
        }
    }

    impl ExistingMasterKeyProvider for FixedKeyProvider {
        fn get_existing(&self) -> Result<Option<MasterKey>> {
            Ok(Some(MasterKey::from_bytes([23; 32])))
        }
    }

    struct FixedKeyProbe(Result<bool>);

    impl KeyReadinessProbe for FixedKeyProbe {
        fn key_is_present(&self) -> Result<bool> {
            match &self.0 {
                Ok(present) => Ok(*present),
                Err(_) => anyhow::bail!("PRIVATE_KEYRING_FAILURE"),
            }
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

    #[test]
    fn native_claude_recording_is_encrypted_silent_and_idempotent() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("SECRET_CLAUDE_PROJECT");
        fs::create_dir(&project).unwrap();
        let payload = serde_json::to_vec(&serde_json::json!({
            "session_id": "SECRET_CLAUDE_SESSION",
            "transcript_path": "/private/SECRET_TRANSCRIPT",
            "cwd": project,
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_use_id": "tool-1",
            "duration_ms": 11,
            "tool_input": {"command": "TOKEN=SECRET_TOKEN false"},
            "error": "PASSWORD=hunter2"
        }))
        .unwrap();
        let data_home = temp.path().join("data");

        let first = record_hook(
            claude::normalize_tool_hook(payload.as_slice()).unwrap(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        let second = record_hook(
            claude::normalize_tool_hook(payload.as_slice()).unwrap(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(first.inserted, 1);
        assert_eq!(second.already_present, 1);

        let database = fs::read(data_home.join("praxis/history.db")).unwrap();
        for forbidden in [
            b"SECRET_CLAUDE_PROJECT".as_slice(),
            b"SECRET_CLAUDE_SESSION",
            b"SECRET_TRANSCRIPT",
            b"SECRET_TOKEN",
            b"hunter2",
        ] {
            assert!(
                !database
                    .windows(forbidden.len())
                    .any(|window| window == forbidden)
            );
        }
    }

    #[test]
    fn native_codex_recording_is_encrypted_silent_and_idempotent() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("SECRET_CODEX_PROJECT");
        fs::create_dir(&project).unwrap();
        let payload = serde_json::to_vec(&serde_json::json!({
            "session_id": "SECRET_CODEX_SESSION",
            "transcript_path": "/private/SECRET_TRANSCRIPT",
            "cwd": project,
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_use_id": "tool-1",
            "duration_ms": 11,
            "tool_input": {"command": "TOKEN=SECRET_TOKEN false"},
            "tool_response": "PASSWORD=hunter2"
        }))
        .unwrap();
        let data_home = temp.path().join("data");

        let first = record_hook(
            codex::normalize_post_tool_observation(payload.as_slice()).unwrap(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        let second = record_hook(
            codex::normalize_post_tool_observation(payload.as_slice()).unwrap(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(first.inserted, 1);
        assert_eq!(second.already_present, 1);

        let database = fs::read(data_home.join("praxis/history.db")).unwrap();
        for forbidden in [
            b"SECRET_CODEX_PROJECT".as_slice(),
            b"SECRET_CODEX_SESSION",
            b"SECRET_TRANSCRIPT",
            b"SECRET_TOKEN",
            b"hunter2",
        ] {
            assert!(
                !database
                    .windows(forbidden.len())
                    .any(|window| window == forbidden)
            );
        }
    }

    #[test]
    fn learned_practice_is_encrypted_ranked_and_read_only_on_recall() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("SECRET_LEARNING_PROJECT");
        fs::create_dir(&project).unwrap();
        let data_home = temp.path().join("data");

        for _ in 0..2 {
            let report = learn_practice(
                project.clone(),
                Strategy::AtomicWrite,
                Outcome::Success,
                data_home.clone(),
                &FixedKeyProvider,
            )
            .unwrap();
            assert_eq!(report.status, crate::application::LearningStatus::Recorded);
        }

        let database_path = data_home.join("praxis/history.db");
        let before_recall = fs::read(&database_path).unwrap();
        let result = recall_project(
            project,
            RecallOptions::default(),
            Some("atomic config write"),
            data_home,
            &FixedKeyProvider,
        )
        .unwrap();

        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].strategy, Some(Strategy::AtomicWrite));
        assert_eq!(result.observations[0].success_rate_percent, Some(100));
        assert_eq!(
            result.observations[0].guidance,
            Some(crate::query::PracticeGuidance::Prefer)
        );
        assert_eq!(fs::read(&database_path).unwrap(), before_recall);
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("SECRET_LEARNING_PROJECT"));
        let database = fs::read(database_path).unwrap();
        assert!(
            !database
                .windows(b"SECRET_LEARNING_PROJECT".len())
                .any(|window| window == b"SECRET_LEARNING_PROJECT")
        );
    }

    #[test]
    fn recall_missing_history_does_not_initialize_state() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("PRIVATE_DATA_HOME");

        let result = recall_project(
            temp.path().to_path_buf(),
            RecallOptions::default(),
            None,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();

        assert!(result.observations.is_empty());
        assert!(!data_home.exists());
    }

    #[test]
    fn generated_claude_config_uses_the_silent_native_handler() {
        assert!(CLAUDE_CONFIG_SNIPPET.contains("praxis record-hook --agent claude"));
        assert!(CLAUDE_CONFIG_SNIPPET.contains("PostToolUseFailure"));
    }

    #[test]
    fn generated_codex_config_uses_the_silent_native_handler() {
        assert!(CODEX_CONFIG_SNIPPET.contains("praxis record-hook --agent codex"));
        assert!(CODEX_CONFIG_SNIPPET.contains("[[hooks.PostToolUse]]"));
    }

    #[test]
    fn health_status_does_not_initialize_or_expose_local_state() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("PRIVATE_DATA_HOME");
        let report = health_status(
            data_home.clone(),
            FixedKeyProbe(Ok(false)),
            std::iter::empty(),
        );
        assert_eq!(
            report.status,
            crate::application::OverallHealth::NotConfigured
        );
        assert!(!data_home.exists());

        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("PRIVATE_DATA_HOME"));
    }

    #[test]
    fn health_status_sanitizes_keyring_and_database_failures() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("data");
        fs::create_dir_all(data_home.join("praxis")).unwrap();
        fs::write(
            data_home.join("praxis/history.db"),
            b"PRIVATE_DATABASE_FAILURE",
        )
        .unwrap();

        let report = health_status(
            data_home,
            FixedKeyProbe(Err(anyhow::anyhow!("PRIVATE_KEYRING_FAILURE"))),
            std::iter::empty(),
        );
        assert_eq!(report.status, crate::application::OverallHealth::Degraded);
        let encoded = serde_json::to_string(&report).unwrap();
        for forbidden in ["PRIVATE_DATABASE_FAILURE", "PRIVATE_KEYRING_FAILURE"] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }

    #[test]
    fn health_status_reports_hook_states_without_paths_or_commands() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("data");
        let report = health_status(
            data_home,
            FixedKeyProbe(Ok(false)),
            [
                (HookProvider::Aoe, Ok(HookReadiness::Installed)),
                (HookProvider::Codex, Ok(HookReadiness::Installed)),
                (HookProvider::Claude, Ok(HookReadiness::Conflicting)),
            ],
        );
        assert_eq!(report.status, crate::application::OverallHealth::Degraded);
        assert_eq!(report.hooks.len(), 3);

        let encoded = serde_json::to_string(&report).unwrap();
        for forbidden in ["config.toml", "settings.json", "praxis record-hook"] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }

    #[test]
    fn forgetting_previews_then_deletes_without_exposing_the_project() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("PRIVATE_FORGOTTEN_PROJECT");
        fs::create_dir(&project).unwrap();
        let data_home = temp.path().join("data");
        let payload = serde_json::to_vec(&serde_json::json!({
            "session_id": "PRIVATE_FORGOTTEN_SESSION",
            "cwd": project,
            "hook_event_name": "PostToolUse",
            "tool_name": "Write",
            "tool_use_id": "tool-forget",
            "tool_input": {"content": "PRIVATE_CONTENT"},
            "tool_response": {"content": "PRIVATE_RESULT"}
        }))
        .unwrap();
        record_hook(
            claude::normalize_tool_hook(payload.as_slice()).unwrap(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();

        let database_path = data_home.join("praxis/history.db");
        let before_preview = fs::read(&database_path).unwrap();
        let preview =
            forget_project(project.clone(), false, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(preview.status, ForgetStatus::Planned);
        assert_eq!(preview.events, 1);
        assert_eq!(fs::read(&database_path).unwrap(), before_preview);
        let still_present = recall_project(
            project.clone(),
            RecallOptions::default(),
            None,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(still_present.observations.len(), 1);

        let forgotten =
            forget_project(project.clone(), true, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(forgotten.status, ForgetStatus::Forgotten);
        assert_eq!(forgotten.events, 1);
        let recalled = recall_project(
            project,
            RecallOptions::default(),
            None,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert!(recalled.observations.is_empty());

        let encoded = serde_json::to_string(&forgotten).unwrap();
        assert!(!encoded.contains("PRIVATE_FORGOTTEN_PROJECT"));
        let database = fs::read(database_path).unwrap();
        for forbidden in [
            b"PRIVATE_FORGOTTEN_PROJECT".as_slice(),
            b"PRIVATE_FORGOTTEN_SESSION",
            b"PRIVATE_CONTENT",
            b"PRIVATE_RESULT",
        ] {
            assert!(
                !database
                    .windows(forbidden.len())
                    .any(|window| window == forbidden)
            );
        }
    }

    #[test]
    fn forgetting_missing_history_does_not_initialize_state() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("PRIVATE_DATA_HOME");
        let report = forget_project(
            temp.path().to_path_buf(),
            false,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(report.status, ForgetStatus::NotFound);
        assert!(!data_home.exists());
    }

    #[test]
    fn retention_previews_then_prunes_with_aggregate_output_only() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("PRIVATE_RETENTION_PROJECT");
        fs::create_dir(&project).unwrap();
        let data_home = temp.path().join("data");
        for tool_use_id in [
            "PRIVATE_EVENT_ONE",
            "PRIVATE_EVENT_TWO",
            "PRIVATE_EVENT_THREE",
        ] {
            let payload = serde_json::to_vec(&serde_json::json!({
                "session_id": "PRIVATE_RETENTION_SESSION",
                "cwd": &project,
                "hook_event_name": "PostToolUse",
                "tool_name": "Write",
                "tool_use_id": tool_use_id,
                "tool_input": {"content": "PRIVATE_RETENTION_CONTENT"},
                "tool_response": {"content": "PRIVATE_RETENTION_RESULT"}
            }))
            .unwrap();
            record_hook(
                claude::normalize_tool_hook(payload.as_slice()).unwrap(),
                data_home.clone(),
                &FixedKeyProvider,
            )
            .unwrap();
        }

        let database_path = data_home.join("praxis/history.db");
        let before_preview = fs::read(&database_path).unwrap();
        let policy = RetentionPolicy::KeepRecent(1)
            .validate(chrono::Utc::now())
            .unwrap();
        let preview = prune_history(policy, false, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(preview.status, RetentionStatus::Planned);
        assert_eq!(preview.events, 2);
        assert_eq!(fs::read(&database_path).unwrap(), before_preview);

        let pruned = prune_history(policy, true, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(pruned.status, RetentionStatus::Pruned);
        assert_eq!(pruned.events, 2);
        let store =
            EncryptedStore::open_read_only(&database_path, &MasterKey::from_bytes([23; 32]))
                .unwrap();
        assert_eq!(store.count().unwrap(), 1);

        let encoded = serde_json::to_string(&pruned).unwrap();
        for forbidden in [
            "PRIVATE_RETENTION_PROJECT",
            "PRIVATE_RETENTION_SESSION",
            "PRIVATE_EVENT",
            "PRIVATE_RETENTION_CONTENT",
        ] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }

    #[test]
    fn retention_missing_history_does_not_initialize_state() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("PRIVATE_DATA_HOME");
        let policy = RetentionPolicy::OlderThanDays(30)
            .validate(chrono::Utc::now())
            .unwrap();
        let report = prune_history(policy, false, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(report.status, RetentionStatus::NoChanges);
        assert!(!data_home.exists());
        assert!(retention_policy(None, None).is_err());
        assert!(retention_policy(Some(30), Some(100)).is_err());
    }
}
