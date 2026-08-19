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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillInstallReport {
    pub status: InstallStatus,
    pub destination: PathBuf,
    pub files: Vec<&'static str>,
}

/// Preview or install the embedded agent-neutral skill under an explicit host
/// skills directory. Existing content is never replaced.
pub fn install_skill(skills_directory: &Path, apply: bool) -> Result<SkillInstallReport> {
    validate_skills_directory(skills_directory)?;
    let destination = skills_directory.join(SKILL_NAME);

    if existing_install_is_current(&destination)? {
        return Ok(report(InstallStatus::AlreadyCurrent, destination));
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
    if existing_install_is_current(&destination)? {
        return Ok(report(InstallStatus::AlreadyCurrent, destination));
    }

    let staging =
        skills_directory.join(format!(".{SKILL_NAME}.install-{}", Uuid::new_v4().simple()));
    if let Err(error) = write_staging_package(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if let Err(rename_error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        if existing_install_is_current(&destination)? {
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

fn existing_install_is_current(destination: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
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
        let file_metadata = fs::symlink_metadata(&path).with_context(|| {
            format!(
                "existing {SKILL_NAME} installation is incomplete at {}",
                path.display()
            )
        })?;
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
            bail!(
                "existing {SKILL_NAME} file differs; refusing to overwrite {}",
                path.display()
            );
        }
    }

    Ok(true)
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
    fn preview_reports_destination_without_writing() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");

        let report = install_skill(&target, false).unwrap();

        assert_eq!(report.status, InstallStatus::Planned);
        assert_eq!(report.destination, target.join(SKILL_NAME));
        assert!(!target.exists());
    }

    #[test]
    fn apply_installs_both_files_and_is_idempotent() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");

        let installed = install_skill(&target, true).unwrap();
        assert_eq!(installed.status, InstallStatus::Installed);
        for packaged in &SKILL_FILES {
            assert_eq!(
                fs::read(installed.destination.join(packaged.relative_path)).unwrap(),
                packaged.contents.as_bytes()
            );
        }

        let repeated = install_skill(&target, true).unwrap();
        assert_eq!(repeated.status, InstallStatus::AlreadyCurrent);
    }

    #[test]
    fn changed_installation_is_preserved() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("agent-skills");
        let installed = install_skill(&target, true).unwrap();
        let skill_file = installed.destination.join("SKILL.md");
        fs::write(&skill_file, "personal change\n").unwrap();

        let error = install_skill(&target, true).unwrap_err();

        assert!(error.to_string().contains("refusing to overwrite"));
        assert_eq!(fs::read_to_string(skill_file).unwrap(), "personal change\n");
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

        let error = install_skill(&target, true).unwrap_err();

        assert!(error.to_string().contains("symlinked skill destination"));
        assert!(fs::read_dir(elsewhere).unwrap().next().is_none());
    }
}
