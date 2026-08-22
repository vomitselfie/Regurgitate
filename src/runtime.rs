use std::{
    io::{self, BufReader, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use zeroize::Zeroizing;

use crate::{
    adapters::{ManagedCodexSource, aoe, aoe_hook, claude, codex},
    application::{
        ExperienceInput, ExperienceReport, ExperienceService, ExperienceSummary, ForgetReport,
        ForgetService, ForgetStatus, HealthReport, HealthService, HookObservation, HookProvider,
        HookReadiness, IngestionReport, IngestionService, ProjectLocator, RecordingReport,
        RecordingService, RetentionPolicy, RetentionReport, RetentionService, RetentionStatus,
        SessionEventSource, TransitionReport, ValidatedRetentionPolicy,
    },
    cli::{Cli, Command, ExperienceCommand, HookAgentArg, PreflightAgentArg},
    core::{
        AgentKind, ApplicabilityTags, Caveat, DebugEvent, EnvironmentFingerprint, Lesson,
        MemoryLifecycle, MemoryScope, Outcome, Procedure, SemanticOutcome, Situation, Strategy,
        TaskKind,
    },
    packaging::{
        AOE_CONFIG_SNIPPET, CLAUDE_CONFIG_SNIPPET, CODEX_CONFIG_SNIPPET, inspect_aoe_hook,
        inspect_claude_hook, inspect_codex_hook, install_aoe_hook, install_claude_hook,
        install_codex_hook, install_skill,
    },
    paths::default_data_home,
    query::{
        EphemeralTaskContext, ExperienceBrief, RecallBroker, RecallOptions, RecallResult,
        RecallService,
    },
    storage::{
        EncryptedStore, ExistingMasterKeyProvider, HistoryDatabaseProbe, MasterKeyProvider,
        SystemKeyProvider, existing_history_database, history_database_for_read,
        prepare_history_database,
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
            task,
            strategy,
            outcome,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = learn_practice(
                project,
                task.into(),
                strategy.into(),
                outcome.into(),
                data_home,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
        Command::Experience { command } => execute_experience(command),
        Command::Preflight {
            project,
            query,
            task,
            phase,
            artifact,
            ecosystem,
            tool_family,
            token_budget,
            agent,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let query = query.map(Zeroizing::new);
            let metadata = EphemeralTaskContext {
                query: None,
                task: task.map(Into::into),
                phase,
                artifact,
                ecosystem,
                tool_family,
            };
            match agent {
                Some(PreflightAgentArg::Claude) => {
                    let request = claude::normalize_prompt_submit(io::stdin().lock())?;
                    let context = EphemeralTaskContext {
                        query: Some(request.prompt.as_str()),
                        ..metadata
                    };
                    let brief = preflight_brief(
                        request.project,
                        context,
                        token_budget,
                        data_home,
                        &SystemKeyProvider::default(),
                    )?;
                    if let Some(response) = claude::preflight_response(&brief.text) {
                        print_json(&response)?;
                    }
                    Ok(())
                }
                None => {
                    let project = project.context("--project is required without --agent")?;
                    let context = EphemeralTaskContext {
                        query: query.as_ref().map(|value| value.as_str()),
                        ..metadata
                    };
                    let brief = preflight_brief(
                        ProjectLocator::new(project),
                        context,
                        token_budget,
                        data_home,
                        &SystemKeyProvider::default(),
                    )?;
                    if !brief.text.is_empty() {
                        let mut stdout = io::stdout().lock();
                        stdout.write_all(brief.text.as_bytes())?;
                        stdout.write_all(b"\n")?;
                    }
                    Ok(())
                }
            }
        }
        Command::BenchReport { runs } => {
            let file = std::fs::File::open(&runs)
                .with_context(|| format!("could not open benchmark runs at {}", runs.display()))?;
            let parsed = crate::bench::parse_runs(BufReader::new(file))?;
            print_json(&crate::bench::summarize(&parsed)?)
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
            task,
            phase,
            artifact,
            ecosystem,
            tool_family,
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
                EphemeralTaskContext {
                    query: query.as_ref().map(|value| value.as_str()),
                    task: task.map(Into::into),
                    phase,
                    artifact,
                    ecosystem,
                    tool_family,
                },
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
    let database = prepare_history_database(&data_home)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&database, &key)?;
    IngestionService::new(source, store).ingest_session(session_id)
}

fn record_hook(
    observation: HookObservation,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<RecordingReport> {
    let database = prepare_history_database(&data_home)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&database, &key)?;
    RecordingService::new(store).record(observation)
}

fn execute_experience(command: ExperienceCommand) -> Result<()> {
    match command {
        ExperienceCommand::Record {
            project,
            scope,
            task,
            situation,
            lesson,
            caveat,
            procedure,
            steps,
            outcome,
            failure_reason,
            phase,
            artifact,
            ecosystem,
            tool_family,
            risks,
            tool_major,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let situation = Zeroizing::new(situation);
            let lesson = Zeroizing::new(lesson);
            let caveat = caveat.map(Zeroizing::new);
            let mut procedure = Procedure::parse_dimensions(&procedure)?;
            if let Some(steps) = steps {
                procedure.steps = Procedure::parse_steps(&steps)?;
            }
            let input = ExperienceInput {
                scope,
                task: task.into(),
                situation: Some(Situation::new(&situation).context("situation rejected")?),
                lesson: Some(Lesson::new(&lesson).context("lesson rejected")?),
                caveat: caveat
                    .as_deref()
                    .map(|text| Caveat::new(text).context("caveat rejected"))
                    .transpose()?,
                procedure,
                outcome: match Outcome::from(outcome) {
                    Outcome::Failure => SemanticOutcome::Failure,
                    _ => SemanticOutcome::Success,
                },
                failure_reason,
                applicability: ApplicabilityTags {
                    artifact_kind: artifact,
                    phase,
                    ecosystem,
                    tool_family,
                    risk_shapes: risks.into_iter().collect(),
                },
                environment: EnvironmentFingerprint {
                    tool_family,
                    major_version: tool_major,
                    host_class: None,
                },
            };
            let report =
                record_experience(project, input, data_home, &SystemKeyProvider::default())?;
            print_json(&report)
        }
        ExperienceCommand::Confirm {
            selector,
            outcome,
            failure_reason,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let outcome = match Outcome::from(outcome) {
                Outcome::Failure => SemanticOutcome::Failure,
                _ => SemanticOutcome::Success,
            };
            let report = confirm_experience(
                &selector,
                outcome,
                failure_reason,
                data_home,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
        ExperienceCommand::List {
            project,
            limit,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let listing =
                list_experiences(project, limit, data_home, &SystemKeyProvider::default())?;
            print_json(&listing)
        }
        ExperienceCommand::Challenge {
            project,
            selector,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = transition_experience(
                project,
                &selector,
                MemoryLifecycle::Challenged,
                data_home,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
        ExperienceCommand::Obsolete {
            project,
            selector,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = transition_experience(
                project,
                &selector,
                MemoryLifecycle::Obsolete,
                data_home,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
        ExperienceCommand::Supersede {
            project,
            old,
            new,
            data_home,
        } => {
            let data_home = data_home.map(Ok).unwrap_or_else(default_data_home)?;
            let report = supersede_experience(
                project,
                &old,
                &new,
                data_home,
                &SystemKeyProvider::default(),
            )?;
            print_json(&report)
        }
    }
}

/// Compatibility shorthand: one controlled strategy becomes a text-free
/// project-scoped capsule so old workflows keep contributing evidence.
fn learn_practice(
    project: PathBuf,
    task: TaskKind,
    strategy: Strategy,
    outcome: Outcome,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<ExperienceReport> {
    let outcome = match outcome {
        Outcome::Success => SemanticOutcome::Success,
        Outcome::Failure => SemanticOutcome::Failure,
        Outcome::Unknown => {
            anyhow::bail!("an explicitly learned practice requires a known outcome")
        }
    };
    record_experience(
        project,
        ExperienceInput {
            scope: MemoryScope::Project,
            task,
            situation: None,
            lesson: None,
            caveat: None,
            procedure: Procedure::from_strategy(strategy),
            outcome,
            failure_reason: None,
            applicability: ApplicabilityTags::default(),
            environment: EnvironmentFingerprint::default(),
        },
        data_home,
        key_provider,
    )
}

fn record_experience(
    project: PathBuf,
    input: ExperienceInput,
    data_home: PathBuf,
    key_provider: &impl MasterKeyProvider,
) -> Result<ExperienceReport> {
    let database = prepare_history_database(&data_home)?;
    let key = key_provider.get_or_create()?;
    let store = EncryptedStore::open(&database, &key)?;
    ExperienceService::new(store).record(ProjectLocator::new(project), input)
}

fn confirm_experience(
    selector: &str,
    outcome: SemanticOutcome,
    failure_reason: Option<crate::core::FailureReason>,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<ExperienceReport> {
    let store = open_existing_history(&data_home, true, key_provider)?
        .context("no Regurgitate history exists yet")?;
    ExperienceService::new(store).confirm(selector, outcome, failure_reason)
}

fn list_experiences(
    project: PathBuf,
    limit: usize,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<Vec<ExperienceSummary>> {
    let Some(store) = open_existing_history(&data_home, true, key_provider)? else {
        return Ok(Vec::new());
    };
    ExperienceService::new(store).list(&ProjectLocator::new(project), limit.clamp(1, 200))
}

fn transition_experience(
    project: PathBuf,
    selector: &str,
    lifecycle: MemoryLifecycle,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<TransitionReport> {
    let store = open_existing_history(&data_home, true, key_provider)?
        .context("no Regurgitate history exists yet")?;
    ExperienceService::new(store).transition(&ProjectLocator::new(project), selector, lifecycle)
}

fn supersede_experience(
    project: PathBuf,
    old: &str,
    new: &str,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<TransitionReport> {
    let store = open_existing_history(&data_home, true, key_provider)?
        .context("no Regurgitate history exists yet")?;
    ExperienceService::new(store).supersede(&ProjectLocator::new(project), old, new)
}

fn preflight_brief(
    project: ProjectLocator,
    context: EphemeralTaskContext<'_>,
    token_budget: usize,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<ExperienceBrief> {
    // Preflight must never fail a host prompt: missing history or key simply
    // yields no brief.
    let Ok(Some(store)) = open_existing_history(&data_home, false, key_provider) else {
        return Ok(ExperienceBrief::empty());
    };
    RecallService::new(&store).brief(&project, context, token_budget)
}

fn health_status(
    data_home: PathBuf,
    key_probe: impl crate::application::KeyReadinessProbe,
    hooks: impl IntoIterator<Item = (HookProvider, Result<HookReadiness>)>,
) -> HealthReport {
    let history = HistoryDatabaseProbe::new(history_database_for_read(&data_home));
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
    let Some(database) = existing_history_database(data_home)? else {
        return Ok(None);
    };
    let key = key_provider
        .get_existing()?
        .context("Regurgitate history exists but its master key is unavailable")?;
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
    context: EphemeralTaskContext<'_>,
    data_home: PathBuf,
    key_provider: &impl ExistingMasterKeyProvider,
) -> Result<RecallResult> {
    options.validate()?;
    let Some(store) = open_existing_history(&data_home, false, key_provider)? else {
        return Ok(RecallResult::empty());
    };
    RecallService::new(&store).recall(&ProjectLocator::new(project), options, context)
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

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
            EphemeralTaskContext::from_query(Some("SUPER_SECRET_QUERY")),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert!(recalled.experiences.is_empty());
        assert_eq!(recalled.hook_summary.sampled_executions, 2);
        let recalled_json = serde_json::to_string(&recalled).unwrap();
        for forbidden in ["aoe-1", "PLAINTEXT_SENTINEL_PROJECT", "SECRET"] {
            assert!(!recalled_json.contains(forbidden));
        }

        let database = fs::read(data_home.join("regurgitate/history.db")).unwrap();
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
        let regurgitate_dir = temp.path().join("data/regurgitate");
        prepare_history_database(&temp.path().join("data")).unwrap();
        let mode = fs::metadata(regurgitate_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn generated_aoe_config_uses_the_identifier_only_handler() {
        assert!(AOE_CONFIG_SNIPPET.contains("on_idle = \"regurgitate aoe-hook\""));
        assert!(AOE_CONFIG_SNIPPET.contains("on_error = \"regurgitate aoe-hook\""));
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
            "cwd": &project,
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

        let database = fs::read(data_home.join("regurgitate/history.db")).unwrap();
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
            "cwd": &project,
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

        let database = fs::read(data_home.join("regurgitate/history.db")).unwrap();
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
                TaskKind::Configuration,
                Strategy::AtomicWrite,
                Outcome::Success,
                data_home.clone(),
                &FixedKeyProvider,
            )
            .unwrap();
            assert!(matches!(
                report.status,
                crate::application::ExperienceStatus::Recorded
                    | crate::application::ExperienceStatus::Confirmed
            ));
        }

        let database_path = data_home.join("regurgitate/history.db");
        let before_recall = fs::read(&database_path).unwrap();
        let result = recall_project(
            project,
            RecallOptions::default(),
            EphemeralTaskContext::from_query(Some("atomic config write")),
            data_home,
            &FixedKeyProvider,
        )
        .unwrap();

        assert_eq!(result.experiences.len(), 1);
        assert_eq!(result.experiences[0].task, TaskKind::Configuration);
        assert_eq!(result.experiences[0].procedure, "atomic-write");
        assert_eq!(result.experiences[0].successes, 2);
        assert_eq!(result.experiences[0].guidance, None);
        assert!(!result.experiences[0].legacy);
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

    fn kicad_input(lesson: &str, outcome: SemanticOutcome) -> ExperienceInput {
        ExperienceInput {
            scope: MemoryScope::Project,
            task: TaskKind::Debugging,
            situation: Some(
                Situation::new(
                    "SITUATION_SENTINEL generated native artifact; parser acceptance is weak.",
                )
                .unwrap(),
            ),
            lesson: Some(Lesson::new(lesson).unwrap()),
            caveat: Some(
                Caveat::new("CAVEAT_SENTINEL serialization success proves nothing.").unwrap(),
            ),
            procedure: Procedure {
                mutation: Some(crate::core::MutationMode::IncrementalNativeRegeneration),
                verification: Some(crate::core::VerificationMode::Native),
                ..Procedure::default()
            },
            outcome,
            failure_reason: None,
            applicability: ApplicabilityTags {
                artifact_kind: Some(crate::core::ArtifactKind::NativeCad),
                phase: Some(crate::core::Phase::Verify),
                ecosystem: Some(crate::core::Ecosystem::Kicad),
                tool_family: Some(crate::core::ToolFamily::Kicad),
                ..ApplicabilityTags::default()
            },
            environment: EnvironmentFingerprint {
                tool_family: Some(crate::core::ToolFamily::Kicad),
                major_version: Some(10),
                host_class: None,
            },
        }
    }

    #[test]
    fn experience_capsules_are_encrypted_confirmed_recalled_and_forgotten() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("SECRET_EXPERIENCE_PROJECT");
        fs::create_dir(&project).unwrap();
        let data_home = temp.path().join("data");
        let lesson = "LESSON_SENTINEL change one placement class at a time, then verify natively.";

        let first = record_experience(
            project.clone(),
            kicad_input(lesson, SemanticOutcome::Success),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(first.status, crate::application::ExperienceStatus::Recorded);
        // A single unconfirmed lesson bootstraps preflight with a ref the
        // agent can confirm.
        let bootstrap = preflight_brief(
            ProjectLocator::new(project.clone()),
            EphemeralTaskContext::from_query(Some("debug generated kicad pcb placement drc")),
            300,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(bootstrap.items, 1);
        assert!(bootstrap.text.contains("[unconfirmed / project]"));
        assert!(bootstrap.text.contains("experience confirm --match"));
        let reference = bootstrap
            .text
            .split("ref ")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .unwrap()
            .to_owned();
        let confirmed = confirm_experience(
            &reference,
            SemanticOutcome::Success,
            None,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(
            confirmed.status,
            crate::application::ExperienceStatus::Confirmed
        );
        assert_eq!(confirmed.evidence, 2);
        for _ in 0..4 {
            let again = record_experience(
                project.clone(),
                kicad_input(lesson, SemanticOutcome::Success),
                data_home.clone(),
                &FixedKeyProvider,
            )
            .unwrap();
            assert_eq!(
                again.status,
                crate::application::ExperienceStatus::Confirmed
            );
        }
        let listing =
            list_experiences(project.clone(), 10, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].successes, 6);
        let listing_json = serde_json::to_string(&listing).unwrap();
        assert!(!listing_json.contains("SENTINEL"));

        let database_path = data_home.join("regurgitate/history.db");
        let before_recall = fs::read(&database_path).unwrap();
        let result = recall_project(
            project.clone(),
            RecallOptions {
                token_budget: crate::query::MAX_TOKEN_BUDGET,
                ..RecallOptions::default()
            },
            EphemeralTaskContext::from_query(Some("debug generated kicad pcb placement drc")),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(fs::read(&database_path).unwrap(), before_recall);
        assert_eq!(result.experiences.len(), 1);
        let item = &result.experiences[0];
        assert_eq!(item.lesson.as_deref(), Some(lesson));
        assert_eq!(item.successes, 6);
        assert_eq!(item.guidance, Some(crate::query::PracticeGuidance::Prefer));
        assert!(!item.legacy);

        let brief = preflight_brief(
            ProjectLocator::new(project.clone()),
            EphemeralTaskContext::from_query(Some("debug generated kicad pcb placement drc")),
            220,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(brief.items, 1);
        assert!(brief.text.contains("LESSON_SENTINEL"));
        assert!(brief.text.contains("Caveat: CAVEAT_SENTINEL"));
        assert!(brief.approximate_tokens <= 220);
        let silent = preflight_brief(
            ProjectLocator::new(project.clone()),
            EphemeralTaskContext::from_query(Some("write the release notes documentation")),
            220,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(silent, ExperienceBrief::empty());

        let database = fs::read(&database_path).unwrap();
        for forbidden in [
            b"SECRET_EXPERIENCE_PROJECT".as_slice(),
            b"SITUATION_SENTINEL",
            b"LESSON_SENTINEL",
            b"CAVEAT_SENTINEL",
            b"kicad",
            b"placement",
        ] {
            assert!(
                !database
                    .windows(forbidden.len())
                    .any(|window| window == forbidden),
                "database leaked {:?}",
                String::from_utf8_lossy(forbidden)
            );
        }

        let obsolete = transition_experience(
            project.clone(),
            &listing[0].selector,
            MemoryLifecycle::Obsolete,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(obsolete.previous, MemoryLifecycle::Active);
        let after = recall_project(
            project.clone(),
            RecallOptions::default(),
            EphemeralTaskContext::default(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert!(after.experiences.is_empty());

        let forgotten =
            forget_project(project.clone(), true, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(forgotten.status, ForgetStatus::Forgotten);
        assert_eq!(forgotten.events, 1);
        assert!(
            list_experiences(project, 10, data_home, &FixedKeyProvider)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn preflight_without_history_or_key_is_silent_and_creates_nothing() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("PRIVATE_DATA_HOME");
        let brief = preflight_brief(
            ProjectLocator::new(temp.path().to_path_buf()),
            EphemeralTaskContext::from_query(Some("anything")),
            220,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(brief, ExperienceBrief::empty());
        assert!(!data_home.exists());
    }

    #[test]
    fn contradicting_experience_challenges_and_supersession_resolves() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let data_home = temp.path().join("data");
        record_experience(
            project.clone(),
            kicad_input(
                "Change one placement class at a time, then verify natively.",
                SemanticOutcome::Success,
            ),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        let report = record_experience(
            project.clone(),
            kicad_input(
                "Regenerate the entire board in bulk and verify once at the end.",
                SemanticOutcome::Success,
            ),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(
            report.status,
            crate::application::ExperienceStatus::Challenged
        );
        let listing =
            list_experiences(project.clone(), 10, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(listing.len(), 2);
        assert!(
            listing
                .iter()
                .all(|summary| summary.lifecycle == MemoryLifecycle::Challenged)
        );

        let newest = &listing[0].selector;
        let oldest = &listing[1].selector;
        let report = supersede_experience(
            project.clone(),
            oldest,
            newest,
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert_eq!(report.lifecycle, MemoryLifecycle::Superseded);
        let listing = list_experiences(project, 10, data_home, &FixedKeyProvider).unwrap();
        let lifecycles: Vec<_> = listing.iter().map(|summary| summary.lifecycle).collect();
        assert!(lifecycles.contains(&MemoryLifecycle::Active));
        assert!(lifecycles.contains(&MemoryLifecycle::Superseded));
    }

    #[test]
    fn recall_missing_history_does_not_initialize_state() {
        let temp = tempdir().unwrap();
        let data_home = temp.path().join("PRIVATE_DATA_HOME");

        let result = recall_project(
            temp.path().to_path_buf(),
            RecallOptions::default(),
            EphemeralTaskContext::default(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();

        assert!(result.experiences.is_empty());
        assert!(!data_home.exists());
    }

    #[test]
    fn generated_claude_config_uses_the_silent_native_handler() {
        assert!(CLAUDE_CONFIG_SNIPPET.contains("regurgitate record-hook --agent claude"));
        assert!(CLAUDE_CONFIG_SNIPPET.contains("PostToolUseFailure"));
    }

    #[test]
    fn generated_codex_config_uses_the_silent_native_handler() {
        assert!(CODEX_CONFIG_SNIPPET.contains("regurgitate record-hook --agent codex"));
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
        fs::create_dir_all(data_home.join("regurgitate")).unwrap();
        fs::write(
            data_home.join("regurgitate/history.db"),
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
        for forbidden in ["config.toml", "settings.json", "regurgitate record-hook"] {
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

        let database_path = data_home.join("regurgitate/history.db");
        let before_preview = fs::read(&database_path).unwrap();
        let preview =
            forget_project(project.clone(), false, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(preview.status, ForgetStatus::Planned);
        assert_eq!(preview.events, 1);
        assert_eq!(fs::read(&database_path).unwrap(), before_preview);
        let still_present = recall_project(
            project.clone(),
            RecallOptions::default(),
            EphemeralTaskContext::default(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert!(still_present.experiences.is_empty());
        assert_eq!(still_present.hook_summary.sampled_executions, 1);

        let forgotten =
            forget_project(project.clone(), true, data_home.clone(), &FixedKeyProvider).unwrap();
        assert_eq!(forgotten.status, ForgetStatus::Forgotten);
        assert_eq!(forgotten.events, 1);
        let recalled = recall_project(
            project,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
            data_home.clone(),
            &FixedKeyProvider,
        )
        .unwrap();
        assert!(recalled.experiences.is_empty());
        assert_eq!(recalled.hook_summary.sampled_executions, 0);

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

        let database_path = data_home.join("regurgitate/history.db");
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
