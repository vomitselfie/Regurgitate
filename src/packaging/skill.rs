use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use uuid::Uuid;

use super::InstallStatus;

const SKILL_NAME: &str = "regurgitate-recall";
const SKILL_CONTENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/skills/regurgitate-recall/SKILL.md"
));
const OPENAI_METADATA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/skills/regurgitate-recall/agents/openai.yaml"
));
const SKILL_PATHS: [&str; 2] = ["SKILL.md", "agents/openai.yaml"];
const PREVIOUS_SKILL_CONTENT: &str = include_str!("recall-skill-v0.10.4.md");
const OLDER_SKILL_CONTENT: &str = include_str!("recall-skill-v0.10.3.md");

struct PackagedFile<'a> {
    relative_path: &'static str,
    contents: &'a str,
}

struct SkillPackage {
    skill: String,
    previous_skill: String,
    older_skill: String,
}

impl SkillPackage {
    fn standard() -> Self {
        Self {
            skill: SKILL_CONTENT.to_owned(),
            previous_skill: PREVIOUS_SKILL_CONTENT.to_owned(),
            older_skill: OLDER_SKILL_CONTENT.to_owned(),
        }
    }

    fn for_command(command: &str) -> Result<Self> {
        if command.is_empty()
            || command
                .chars()
                .any(|character| character.is_control() || character == '`')
        {
            bail!(
                "Regurgitate skill command must be non-empty, single-line, and contain no backticks"
            );
        }
        let heading = "# Regurgitate Recall\n";
        let instruction = format!(
            "{heading}\nReplace the leading `regurgitate` in every command and approval prefix below with `{command}`; invoke it directly, never through a shell wrapper.\n"
        );
        Ok(Self {
            skill: SKILL_CONTENT.replacen(heading, &instruction, 1),
            previous_skill: PREVIOUS_SKILL_CONTENT.replacen(heading, &instruction, 1),
            older_skill: OLDER_SKILL_CONTENT.replacen(heading, &instruction, 1),
        })
    }

    fn files(&self) -> [PackagedFile<'_>; 2] {
        [
            PackagedFile {
                relative_path: SKILL_PATHS[0],
                contents: &self.skill,
            },
            PackagedFile {
                relative_path: SKILL_PATHS[1],
                contents: OPENAI_METADATA,
            },
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingInstall {
    Missing,
    Current,
    CompatibleStandard,
    Different,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillInstallReport {
    pub status: InstallStatus,
    pub destination: PathBuf,
    pub files: Vec<&'static str>,
}

/// Preview or install the embedded agent-neutral skill under an explicit host
/// skills directory. Different existing content is replaced only when the
/// caller explicitly opts in.
pub fn install_skill(
    skills_directory: &Path,
    apply: bool,
    replace: bool,
) -> Result<SkillInstallReport> {
    install_skill_package(skills_directory, apply, replace, &SkillPackage::standard())
}

/// Install the same bounded skill with every Regurgitate invocation pinned to one
/// executable. AoE uses this because release-binary workers are not on PATH.
pub fn install_skill_with_command(
    skills_directory: &Path,
    command: &str,
    apply: bool,
    replace: bool,
) -> Result<SkillInstallReport> {
    let package = SkillPackage::for_command(command)?;
    install_skill_package(skills_directory, apply, replace, &package)
}

/// Render one executable path as a single shell command token for an agent
/// instruction. The path is never executed here.
pub fn quote_agent_executable(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .context("Regurgitate executable path is not valid UTF-8")?;
    if path.is_empty()
        || path
            .chars()
            .any(|character| character.is_control() || character == '`')
    {
        bail!("Regurgitate executable path cannot be represented safely in an agent command");
    }
    Ok(format!("'{}'", path.replace('\'', "'\"'\"'")))
}

fn install_skill_package(
    skills_directory: &Path,
    apply: bool,
    replace: bool,
    package: &SkillPackage,
) -> Result<SkillInstallReport> {
    validate_skills_directory(skills_directory)?;
    let destination = skills_directory.join(SKILL_NAME);

    match inspect_existing_install(&destination, package)? {
        ExistingInstall::Current => {
            return Ok(report(InstallStatus::AlreadyCurrent, destination));
        }
        ExistingInstall::Different if !replace => {
            bail!("existing {SKILL_NAME} installation differs; preview replacement with --replace");
        }
        ExistingInstall::Missing
        | ExistingInstall::CompatibleStandard
        | ExistingInstall::Different => {}
    }

    if !apply {
        return Ok(report(InstallStatus::Planned, destination));
    }

    fs::create_dir_all(skills_directory).with_context(|| {
        format!(
            "could not create the skills directory at {}",
            skills_directory.display()
        )
    })?;

    // Recheck after creating the parent so concurrent installers cannot turn a
    // preview into an overwrite.
    let existing = inspect_existing_install(&destination, package)?;
    match existing {
        ExistingInstall::Current => {
            return Ok(report(InstallStatus::AlreadyCurrent, destination));
        }
        ExistingInstall::Different if !replace => {
            bail!("existing {SKILL_NAME} installation differs; preview replacement with --replace");
        }
        ExistingInstall::Missing
        | ExistingInstall::CompatibleStandard
        | ExistingInstall::Different => {}
    }

    let staging =
        skills_directory.join(format!(".{SKILL_NAME}.install-{}", Uuid::new_v4().simple()));
    if let Err(error) = write_staging_package(&staging, package) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if matches!(
        existing,
        ExistingInstall::CompatibleStandard | ExistingInstall::Different
    ) {
        replace_staging_package(skills_directory, &staging, &destination)?;
        return Ok(report(InstallStatus::Replaced, destination));
    }

    if let Err(rename_error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        if inspect_existing_install(&destination, package)? == ExistingInstall::Current {
            return Ok(report(InstallStatus::AlreadyCurrent, destination));
        }
        return Err(rename_error).with_context(|| {
            format!(
                "could not install {SKILL_NAME} at {}",
                destination.display()
            )
        });
    }

    Ok(report(InstallStatus::Installed, destination))
}

fn validate_skills_directory(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => bail!("skills target is not a directory: {}", path.display()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("could not inspect skills target at {}", path.display())),
    }
}

fn inspect_existing_install(destination: &Path, package: &SkillPackage) -> Result<ExistingInstall> {
    let metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(ExistingInstall::Missing),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect the existing skill at {}",
                    destination.display()
                )
            });
        }
    };

    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to replace symlinked skill destination: {}",
            destination.display()
        );
    }
    if !metadata.is_dir() {
        bail!(
            "refusing to replace non-directory skill destination: {}",
            destination.display()
        );
    }

    let mut compatible_standard = false;
    for packaged in package.files() {
        let path = destination.join(packaged.relative_path);
        let file_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(ExistingInstall::Different);
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "could not inspect installed skill file at {}",
                        path.display()
                    )
                });
            }
        };
        if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
            bail!(
                "existing {SKILL_NAME} file is not a regular file: {}",
                path.display()
            );
        }
        let current = fs::read(&path).with_context(|| {
            format!("could not read installed skill file at {}", path.display())
        })?;
        let standard_skill_is_compatible = packaged.relative_path == "SKILL.md"
            && ((package.skill != SKILL_CONTENT && current == SKILL_CONTENT.as_bytes())
                || current == PREVIOUS_SKILL_CONTENT.as_bytes()
                || current == package.previous_skill.as_bytes()
                || current == OLDER_SKILL_CONTENT.as_bytes()
                || current == package.older_skill.as_bytes());
        if standard_skill_is_compatible {
            compatible_standard = true;
        } else if current != packaged.contents.as_bytes() {
            return Ok(ExistingInstall::Different);
        }
    }

    Ok(
        if compatible_standard && has_only_packaged_layout(destination)? {
            ExistingInstall::CompatibleStandard
        } else if compatible_standard {
            ExistingInstall::Different
        } else {
            ExistingInstall::Current
        },
    )
}

fn has_only_packaged_layout(destination: &Path) -> Result<bool> {
    let mut root_entries = fs::read_dir(destination)
        .with_context(|| format!("could not inspect installed {SKILL_NAME} directory"))?;
    let mut saw_skill = false;
    let mut saw_agents = false;
    for entry in &mut root_entries {
        let entry = entry.context("could not inspect installed skill entry")?;
        match entry.file_name().to_str() {
            Some("SKILL.md") if entry.file_type()?.is_file() => saw_skill = true,
            Some("agents") if entry.file_type()?.is_dir() => saw_agents = true,
            _ => return Ok(false),
        }
    }
    if !saw_skill || !saw_agents {
        return Ok(false);
    }

    let mut agent_entries = fs::read_dir(destination.join("agents"))
        .context("could not inspect installed skill agent metadata")?;
    let mut saw_metadata = false;
    for entry in &mut agent_entries {
        let entry = entry.context("could not inspect installed skill agent metadata entry")?;
        match entry.file_name().to_str() {
            Some("openai.yaml") if entry.file_type()?.is_file() => saw_metadata = true,
            _ => return Ok(false),
        }
    }
    Ok(saw_metadata)
}

fn replace_staging_package(
    skills_directory: &Path,
    staging: &Path,
    destination: &Path,
) -> Result<()> {
    let backup = skills_directory.join(format!(".{SKILL_NAME}.backup-{}", Uuid::new_v4().simple()));
    fs::rename(destination, &backup).with_context(|| {
        format!("could not stage the existing {SKILL_NAME} installation for replacement")
    })?;

    if let Err(install_error) = fs::rename(staging, destination) {
        let restore_result = fs::rename(&backup, destination);
        let _ = fs::remove_dir_all(staging);
        if restore_result.is_err() {
            bail!("could not install or restore the existing {SKILL_NAME} installation");
        }
        return Err(install_error).context("could not replace the existing skill installation");
    }

    fs::remove_dir_all(&backup)
        .context("installed the new skill but could not remove its private backup")?;
    Ok(())
}

fn write_staging_package(staging: &Path, package: &SkillPackage) -> Result<()> {
    fs::create_dir(staging).with_context(|| {
        format!(
            "could not create temporary skill directory at {}",
            staging.display()
        )
    })?;

    for packaged in package.files() {
        let path = staging.join(packaged.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "could not create skill package directory at {}",
                    parent.display()
                )
            })?;
        }
        fs::write(&path, packaged.contents)
            .with_context(|| format!("could not stage skill file at {}", path.display()))?;
    }
    Ok(())
}

fn report(status: InstallStatus, destination: PathBuf) -> SkillInstallReport {
    SkillInstallReport {
        status,
        destination,
        files: SKILL_PATHS.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn skipping_a_release_still_upgrades_the_older_stock_skill() {
        for command in [None, Some("'/opt/regurgitate'")] {
            let temp = tempdir().unwrap();
            let package = match command {
                Some(command) => SkillPackage::for_command(command).unwrap(),
                None => SkillPackage::standard(),
            };
            let destination = temp.path().join(SKILL_NAME);
            write_staging_package(&destination, &package).unwrap();
            fs::write(destination.join("SKILL.md"), &package.older_skill).unwrap();
            assert_eq!(
                install_skill_package(temp.path(), true, false, &package)
                    .unwrap()
                    .status,
                InstallStatus::Replaced
            );
            assert_eq!(
                fs::read_to_string(destination.join("SKILL.md")).unwrap(),
                package.skill
            );
        }
    }

    #[test]
    fn previous_stock_skills_upgrade_but_personal_files_prevent_replacement() {
        for command in [None, Some("'/opt/regurgitate'")] {
            for extra in [None, Some("NOTES.md"), Some("agents/personal.yaml")] {
                let temp = tempdir().unwrap();
                let package = match command {
                    Some(command) => SkillPackage::for_command(command).unwrap(),
                    None => SkillPackage::standard(),
                };
                let destination = temp.path().join(SKILL_NAME);
                write_staging_package(&destination, &package).unwrap();
                fs::write(destination.join("SKILL.md"), &package.previous_skill).unwrap();
                if let Some(extra) = extra {
                    fs::write(destination.join(extra), "personal content").unwrap();
                    assert!(install_skill_package(temp.path(), true, false, &package).is_err());
                    assert_eq!(
                        fs::read_to_string(destination.join(extra)).unwrap(),
                        "personal content"
                    );
                    assert_eq!(
                        fs::read_to_string(destination.join("SKILL.md")).unwrap(),
                        package.previous_skill
                    );
                } else {
                    assert_eq!(
                        install_skill_package(temp.path(), false, false, &package)
                            .unwrap()
                            .status,
                        InstallStatus::Planned
                    );
                    assert_eq!(
                        install_skill_package(temp.path(), true, false, &package)
                            .unwrap()
                            .status,
                        InstallStatus::Replaced
                    );
                    assert_eq!(
                        fs::read_to_string(destination.join("SKILL.md")).unwrap(),
                        package.skill
                    );
                }
            }
        }
    }

    #[test]
    fn agent_instruction_bundle_stays_compact() {
        assert!(
            SKILL_CONTENT.len() <= 3_000,
            "SKILL.md should stay below its 750-token conservative byte budget"
        );
    }

    #[test]
    fn preview_reports_destination_without_writing() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");

        let report = install_skill(&target, false, false).unwrap();

        assert_eq!(report.status, InstallStatus::Planned);
        assert_eq!(report.destination, target.join(SKILL_NAME));
        assert!(!target.exists());
    }

    #[test]
    fn apply_installs_both_files_and_is_idempotent() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");

        let installed = install_skill(&target, true, false).unwrap();
        assert_eq!(installed.status, InstallStatus::Installed);
        let package = SkillPackage::standard();
        for packaged in package.files() {
            assert_eq!(
                fs::read(installed.destination.join(packaged.relative_path)).unwrap(),
                packaged.contents.as_bytes()
            );
        }

        let repeated = install_skill(&target, true, false).unwrap();
        assert_eq!(repeated.status, InstallStatus::AlreadyCurrent);
    }

    #[test]
    fn changed_installation_is_preserved() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        let installed = install_skill(&target, true, false).unwrap();
        let skill_file = installed.destination.join("SKILL.md");
        fs::write(&skill_file, "personal change\n").unwrap();

        let error = install_skill(&target, true, false).unwrap_err();

        assert!(error.to_string().contains("preview replacement"));
        assert_eq!(fs::read_to_string(skill_file).unwrap(), "personal change\n");
    }

    #[test]
    fn explicit_replacement_previews_then_atomically_updates() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        let installed = install_skill(&target, true, false).unwrap();
        let skill_file = installed.destination.join("SKILL.md");
        fs::write(&skill_file, "previous bundle or personal change\n").unwrap();

        let preview = install_skill(&target, false, true).unwrap();
        assert_eq!(preview.status, InstallStatus::Planned);
        assert_eq!(
            fs::read_to_string(&skill_file).unwrap(),
            "previous bundle or personal change\n"
        );

        let replaced = install_skill(&target, true, true).unwrap();
        assert_eq!(replaced.status, InstallStatus::Replaced);
        assert_eq!(fs::read(&skill_file).unwrap(), SKILL_CONTENT.as_bytes());
        assert!(fs::read_dir(&target).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        }));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_destination_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        let elsewhere = temp.path().join("elsewhere");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir(&elsewhere).unwrap();
        symlink(&elsewhere, target.join(SKILL_NAME)).unwrap();

        let error = install_skill(&target, true, true).unwrap_err();

        assert!(error.to_string().contains("symlinked skill destination"));
        assert!(fs::read_dir(elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn explicit_command_is_rendered_without_changing_the_source_bundle() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        let command = "'/plugin home/regurgitate'";

        let installed = install_skill_with_command(&target, command, true, false).unwrap();

        let rendered = fs::read_to_string(installed.destination.join("SKILL.md")).unwrap();
        assert!(rendered.contains(
            "Replace the leading `regurgitate` in every command and approval prefix below with `'/plugin home/regurgitate'`; invoke it directly, never through a shell wrapper."
        ));
        assert!(rendered.contains("regurgitate recall"));
        assert!(rendered.contains("regurgitate experience confirm"));
        assert!(!SKILL_CONTENT.contains(command));
    }

    #[test]
    fn pinned_command_safely_migrates_an_untouched_standard_skill() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        install_skill(&target, true, false).unwrap();

        let preview = install_skill_with_command(
            &target,
            "'/home/user/.local/bin/regurgitate'",
            false,
            false,
        )
        .unwrap();
        assert_eq!(preview.status, InstallStatus::Planned);

        let migrated =
            install_skill_with_command(&target, "'/home/user/.local/bin/regurgitate'", true, false)
                .unwrap();
        assert_eq!(migrated.status, InstallStatus::Replaced);
        let rendered = fs::read_to_string(migrated.destination.join("SKILL.md")).unwrap();
        assert!(rendered.contains("'/home/user/.local/bin/regurgitate'"));
    }

    #[test]
    fn pinned_command_preserves_a_standard_skill_with_extra_personal_files() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        install_skill(&target, true, false).unwrap();
        let personal = target.join(SKILL_NAME).join("NOTES.md");
        fs::write(&personal, "personal notes\n").unwrap();

        let error =
            install_skill_with_command(&target, "'/home/user/.local/bin/regurgitate'", true, false)
                .unwrap_err();

        assert!(error.to_string().contains("preview replacement"));
        assert_eq!(fs::read_to_string(personal).unwrap(), "personal notes\n");
    }

    #[test]
    fn agent_executable_paths_are_quoted_without_command_injection() {
        let quoted = quote_agent_executable(Path::new("/user's tools/regurgitate")).unwrap();
        assert_eq!(quoted, "'/user'\"'\"'s tools/regurgitate'");
        assert!(quote_agent_executable(Path::new("/tmp/`unsafe`")).is_err());
        assert!(quote_agent_executable(Path::new("/tmp/unsafe\npath")).is_err());
    }
}
