use std::{env, path::PathBuf};

use anyhow::{Context, Result};

use crate::{
    application::HookReadiness,
    packaging::{
        InstallStatus, inspect_claude_hook_command, inspect_codex_hook_command,
        install_claude_hook_command, install_codex_hook_command, install_skill_with_command,
        quote_agent_executable,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupTarget {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupOutcome {
    Installed,
    AlreadyCurrent,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IntegrationReadiness {
    Ready,
    NotConfigured,
    NeedsAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IntegrationState {
    pub hook: IntegrationReadiness,
    pub skill: IntegrationReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IntegrationOverview {
    pub codex: IntegrationState,
    pub claude: IntegrationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SetupNotice {
    pub target: SetupTarget,
    pub outcome: SetupOutcome,
}

#[derive(Debug, Clone)]
struct SetupPaths {
    codex_config: PathBuf,
    codex_skills: PathBuf,
    claude_config: PathBuf,
    claude_skills: PathBuf,
}

impl SetupPaths {
    fn from_environment() -> Result<Self> {
        let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
        let codex_home = env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let claude_home = env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        Ok(Self {
            codex_config: codex_home.join("config.toml"),
            codex_skills: codex_home.join("skills"),
            claude_config: claude_home.join("settings.json"),
            claude_skills: claude_home.join("skills"),
        })
    }
}

pub(super) struct SetupService {
    executable_command: String,
    paths: SetupPaths,
}

impl SetupService {
    pub fn from_environment() -> Result<Self> {
        let executable =
            env::current_exe().context("could not locate the Regurgitate plugin binary")?;
        Ok(Self {
            executable_command: quote_agent_executable(&executable)?,
            paths: SetupPaths::from_environment()?,
        })
    }

    #[cfg(test)]
    fn new(executable: &str, paths: SetupPaths) -> Result<Self> {
        Ok(Self {
            executable_command: quote_agent_executable(std::path::Path::new(executable))?,
            paths,
        })
    }

    pub fn inspect(&self) -> IntegrationOverview {
        IntegrationOverview {
            codex: self.inspect_target(SetupTarget::Codex),
            claude: self.inspect_target(SetupTarget::Claude),
        }
    }

    pub fn setup(&self, target: SetupTarget) -> Result<SetupOutcome> {
        // Validate both destinations before either is changed. A concurrent
        // writer can still cause a partial safe install, which inspect exposes.
        self.preview_hook(target)?;
        self.preview_skill(target)?;

        let skill = install_skill_with_command(
            self.skills_path(target),
            &self.executable_command,
            true,
            false,
        )?;
        let hook = self.install_hook(target, true)?;
        let changed = changed(skill.status) || changed(hook);
        Ok(if changed {
            SetupOutcome::Installed
        } else {
            SetupOutcome::AlreadyCurrent
        })
    }

    fn inspect_target(&self, target: SetupTarget) -> IntegrationState {
        let hook_command = self.hook_command(target);
        let hook = match target {
            SetupTarget::Codex => {
                inspect_codex_hook_command(&self.paths.codex_config, &hook_command)
            }
            SetupTarget::Claude => {
                inspect_claude_hook_command(&self.paths.claude_config, &hook_command)
            }
        };
        let skill = install_skill_with_command(
            self.skills_path(target),
            &self.executable_command,
            false,
            false,
        );
        IntegrationState {
            hook: hook
                .map(hook_readiness)
                .unwrap_or(IntegrationReadiness::NeedsAttention),
            skill: match skill {
                Ok(report) if report.status == InstallStatus::AlreadyCurrent => {
                    IntegrationReadiness::Ready
                }
                Ok(_) => IntegrationReadiness::NotConfigured,
                Err(_) => IntegrationReadiness::NeedsAttention,
            },
        }
    }

    fn preview_hook(&self, target: SetupTarget) -> Result<()> {
        self.install_hook(target, false).map(|_| ())
    }

    fn preview_skill(&self, target: SetupTarget) -> Result<()> {
        install_skill_with_command(
            self.skills_path(target),
            &self.executable_command,
            false,
            false,
        )
        .map(|_| ())
    }

    fn install_hook(&self, target: SetupTarget, apply: bool) -> Result<InstallStatus> {
        let command = self.hook_command(target);
        match target {
            SetupTarget::Codex => {
                install_codex_hook_command(&self.paths.codex_config, &command, apply)
                    .map(|report| report.status)
            }
            SetupTarget::Claude => {
                install_claude_hook_command(&self.paths.claude_config, &command, apply)
                    .map(|report| report.status)
            }
        }
    }

    fn hook_command(&self, target: SetupTarget) -> String {
        let agent = match target {
            SetupTarget::Codex => "codex",
            SetupTarget::Claude => "claude",
        };
        format!("{} record-hook --agent {agent}", self.executable_command)
    }

    fn skills_path(&self, target: SetupTarget) -> &std::path::Path {
        match target {
            SetupTarget::Codex => &self.paths.codex_skills,
            SetupTarget::Claude => &self.paths.claude_skills,
        }
    }
}

fn hook_readiness(readiness: HookReadiness) -> IntegrationReadiness {
    match readiness {
        HookReadiness::Installed => IntegrationReadiness::Ready,
        HookReadiness::NotInstalled => IntegrationReadiness::NotConfigured,
        HookReadiness::Conflicting | HookReadiness::Unavailable => {
            IntegrationReadiness::NeedsAttention
        }
    }
}

fn changed(status: InstallStatus) -> bool {
    matches!(status, InstallStatus::Installed | InstallStatus::Replaced)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn fixture_paths(root: &std::path::Path) -> SetupPaths {
        SetupPaths {
            codex_config: root.join("codex/config.toml"),
            codex_skills: root.join("codex/skills"),
            claude_config: root.join("claude/settings.json"),
            claude_skills: root.join("claude/skills"),
        }
    }

    #[test]
    fn codex_setup_is_complete_and_idempotent_without_path_lookup() {
        let temp = tempdir().unwrap();
        let service =
            SetupService::new("/plugin home/regurgitate", fixture_paths(temp.path())).unwrap();

        assert_eq!(
            service.setup(SetupTarget::Codex).unwrap(),
            SetupOutcome::Installed
        );
        assert_eq!(service.inspect().codex.hook, IntegrationReadiness::Ready);
        assert_eq!(service.inspect().codex.skill, IntegrationReadiness::Ready);
        assert_eq!(
            service.setup(SetupTarget::Codex).unwrap(),
            SetupOutcome::AlreadyCurrent
        );

        let config = fs::read_to_string(&service.paths.codex_config).unwrap();
        let skill = fs::read_to_string(
            service
                .paths
                .codex_skills
                .join("regurgitate-recall/SKILL.md"),
        )
        .unwrap();
        assert!(config.contains("'/plugin home/regurgitate' record-hook --agent codex"));
        assert!(skill.contains(
            "Replace the leading `regurgitate` in every command and approval prefix below with `'/plugin home/regurgitate'`"
        ));
        assert!(skill.contains("regurgitate recall"));
    }

    #[test]
    fn claude_setup_preserves_personal_settings() {
        let temp = tempdir().unwrap();
        let paths = fixture_paths(temp.path());
        fs::create_dir_all(paths.claude_config.parent().unwrap()).unwrap();
        fs::write(
            &paths.claude_config,
            r#"{"model":"PRIVATE_MODEL","hooks":{"Stop":[{"hooks":[]}]}}"#,
        )
        .unwrap();
        let service = SetupService::new("/plugins/regurgitate", paths).unwrap();

        assert_eq!(
            service.setup(SetupTarget::Claude).unwrap(),
            SetupOutcome::Installed
        );

        let config = fs::read_to_string(&service.paths.claude_config).unwrap();
        assert!(config.contains("PRIVATE_MODEL"));
        assert!(config.contains("\"Stop\""));
        assert!(config.contains("'/plugins/regurgitate' record-hook --agent claude"));
        assert_eq!(service.inspect().claude.hook, IntegrationReadiness::Ready);
        assert_eq!(service.inspect().claude.skill, IntegrationReadiness::Ready);
    }

    #[test]
    fn conflict_is_detected_before_skill_install() {
        let temp = tempdir().unwrap();
        let paths = fixture_paths(temp.path());
        fs::create_dir_all(paths.codex_config.parent().unwrap()).unwrap();
        fs::write(&paths.codex_config, "[features]\nhooks = false\n").unwrap();
        let service = SetupService::new("/plugins/regurgitate", paths).unwrap();

        assert!(service.setup(SetupTarget::Codex).is_err());
        assert!(!service.paths.codex_skills.exists());
        assert_eq!(
            service.inspect().codex.hook,
            IntegrationReadiness::NeedsAttention
        );
    }

    #[test]
    fn executable_paths_are_shell_quoted_without_allowing_command_syntax() {
        assert_eq!(
            quote_agent_executable(std::path::Path::new("/plugin's home/regurgitate")).unwrap(),
            r#"'/plugin'"'"'s home/regurgitate'"#
        );
        assert!(
            quote_agent_executable(std::path::Path::new("/plugin/`private`/regurgitate")).is_err()
        );
        assert!(quote_agent_executable(std::path::Path::new("/plugin\nhome/regurgitate")).is_err());
    }
}
