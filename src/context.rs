//! Bounded, content-free project context detection for agent ergonomics.
//!
//! Detection checks only an allowlist of marker paths. It never enumerates a
//! directory, reads a marker, invokes a project tool, or retains a path.

use std::{fs, path::Path};

use crate::{
    core::{ArtifactKind, Ecosystem, ToolFamily},
    query::ProjectDefaults,
};

const MAX_CONTEXT_ANCESTORS: usize = 32;

pub(crate) fn infer_project_defaults(path: &Path) -> ProjectDefaults {
    let start = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    for directory in start.ancestors().take(MAX_CONTEXT_ANCESTORS) {
        if let Some(context) = infer_at(directory) {
            return context;
        }
        if marker_exists(directory, ".git") {
            break;
        }
    }
    ProjectDefaults::default()
}

fn infer_at(directory: &Path) -> Option<ProjectDefaults> {
    if regular_marker(directory, "Cargo.toml") {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Source),
            ecosystem: Some(Ecosystem::Rust),
            tool_family: Some(ToolFamily::Cargo),
        });
    }
    if regular_marker(directory, "tsconfig.json") {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Source),
            ecosystem: Some(Ecosystem::Typescript),
            tool_family: node_tool(directory),
        });
    }
    if regular_marker(directory, "package.json") {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Source),
            ecosystem: Some(Ecosystem::Javascript),
            tool_family: node_tool(directory),
        });
    }
    if regular_marker(directory, "pyproject.toml")
        || regular_marker(directory, "setup.py")
        || regular_marker(directory, "requirements.txt")
    {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Source),
            ecosystem: Some(Ecosystem::Python),
            tool_family: regular_marker(directory, "pytest.ini").then_some(ToolFamily::Pytest),
        });
    }
    if regular_marker(directory, "go.mod") {
        return Some(source(Ecosystem::Go, None));
    }
    if regular_marker(directory, "pom.xml") || regular_marker(directory, "build.gradle") {
        return Some(source(Ecosystem::Java, None));
    }
    if regular_marker(directory, "CMakeLists.txt") {
        return Some(source(Ecosystem::Cpp, Some(ToolFamily::Cmake)));
    }
    if regular_marker(directory, "Makefile") {
        return Some(source(Ecosystem::Generic, Some(ToolFamily::Make)));
    }
    if regular_marker(directory, "Dockerfile")
        || regular_marker(directory, "compose.yaml")
        || regular_marker(directory, "docker-compose.yml")
    {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Config),
            ecosystem: Some(Ecosystem::Docker),
            tool_family: Some(ToolFamily::Docker),
        });
    }
    if regular_marker(directory, ".terraform.lock.hcl") {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Config),
            ecosystem: Some(Ecosystem::Generic),
            tool_family: Some(ToolFamily::Terraform),
        });
    }
    if regular_marker(directory, "mkdocs.yml") || regular_marker(directory, "docs/conf.py") {
        return Some(ProjectDefaults {
            artifact: Some(ArtifactKind::Docs),
            ecosystem: None,
            tool_family: None,
        });
    }
    None
}

fn source(ecosystem: Ecosystem, tool_family: Option<ToolFamily>) -> ProjectDefaults {
    ProjectDefaults {
        artifact: Some(ArtifactKind::Source),
        ecosystem: Some(ecosystem),
        tool_family,
    }
}

fn node_tool(directory: &Path) -> Option<ToolFamily> {
    if regular_marker(directory, "pnpm-lock.yaml") {
        Some(ToolFamily::Pnpm)
    } else if regular_marker(directory, "yarn.lock") {
        Some(ToolFamily::Yarn)
    } else {
        Some(ToolFamily::Npm)
    }
}

fn marker_exists(directory: &Path, marker: &str) -> bool {
    fs::symlink_metadata(directory.join(marker)).is_ok()
}

fn regular_marker(directory: &Path, marker: &str) -> bool {
    fs::symlink_metadata(directory.join(marker)).is_ok_and(|metadata| metadata.is_file())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn infers_nearest_allowlisted_manifest_without_reading_it() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join("Cargo.toml"), b"PRIVATE_MANIFEST_CONTENT").unwrap();
        let nested = temp.path().join("src/query");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(
            infer_project_defaults(&nested),
            ProjectDefaults {
                artifact: Some(ArtifactKind::Source),
                ecosystem: Some(Ecosystem::Rust),
                tool_family: Some(ToolFamily::Cargo),
            }
        );
    }

    #[test]
    fn detects_node_package_manager_and_ignores_symlinked_markers() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("package.json"), b"PRIVATE_PACKAGE_CONTENT").unwrap();
        fs::write(temp.path().join("pnpm-lock.yaml"), b"PRIVATE_LOCK_CONTENT").unwrap();
        assert_eq!(
            infer_project_defaults(temp.path()).tool_family,
            Some(ToolFamily::Pnpm)
        );

        let other = tempdir().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            temp.path().join("package.json"),
            other.path().join("package.json"),
        )
        .unwrap();
        #[cfg(unix)]
        assert_eq!(
            infer_project_defaults(other.path()),
            ProjectDefaults::default()
        );
    }
}
