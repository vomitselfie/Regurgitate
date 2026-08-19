use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use uuid::Uuid;

use super::InstallStatus;

const SKILL_NAME: &str = "praxis-recall";
const SKILL_FILES: [PackagedFile; 2] = [
    PackagedFile {
        relative_path: "SKILL.md",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/praxis-recall/SKILL.md"
        )),
    },
    PackagedFile {
        relative_path: "agents/openai.yaml",
        contents: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/praxis-recall/agents/openai.yaml"
        )),
    },
];

struct PackagedFile {
    relative_path: &'static str,
    contents: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingInstall {
    Missing,
    Current,
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
    validate_skills_directory(skills_directory)?;
    let destination = skills_directory.join(SKILL_NAME);

    match inspect_existing_install(&destination)? {
        ExistingInstall::Current => {
            return Ok(report(InstallStatus::AlreadyCurrent, destination));
        }
        ExistingInstall::Different if !replace => {
            bail!("existing {SKILL_NAME} installation differs; preview replacement with --replace");
        }
        ExistingInstall::Missing | ExistingInstall::Different => {}
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
    let existing = inspect_existing_install(&destination)?;
    match existing {
        ExistingInstall::Current => {
            return Ok(report(InstallStatus::AlreadyCurrent, destination));
        }
        ExistingInstall::Different if !replace => {
            bail!("existing {SKILL_NAME} installation differs; preview replacement with --replace");
        }
        ExistingInstall::Missing | ExistingInstall::Different => {}
    }

    let staging =
        skills_directory.join(format!(".{SKILL_NAME}.install-{}", Uuid::new_v4().simple()));
    if let Err(error) = write_staging_package(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if existing == ExistingInstall::Different {
        replace_staging_package(skills_directory, &staging, &destination)?;
        return Ok(report(InstallStatus::Replaced, destination));
    }

    if let Err(rename_error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        if inspect_existing_install(&destination)? == ExistingInstall::Current {
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

fn inspect_existing_install(destination: &Path) -> Result<ExistingInstall> {
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

    for packaged in &SKILL_FILES {
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
        if current != packaged.contents.as_bytes() {
            return Ok(ExistingInstall::Different);
        }
    }

    Ok(ExistingInstall::Current)
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

fn write_staging_package(staging: &Path) -> Result<()> {
    fs::create_dir(staging).with_context(|| {
        format!(
            "could not create temporary skill directory at {}",
            staging.display()
        )
    })?;

    for packaged in &SKILL_FILES {
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
        files: SKILL_FILES.iter().map(|file| file.relative_path).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn agent_instruction_bundle_stays_compact() {
        assert!(
            SKILL_FILES[0].contents.len() <= 3_000,
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
        for packaged in &SKILL_FILES {
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
        assert_eq!(
            fs::read(&skill_file).unwrap(),
            SKILL_FILES[0].contents.as_bytes()
        );
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
}
