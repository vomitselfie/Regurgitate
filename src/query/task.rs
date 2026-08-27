use std::collections::BTreeSet;

use zeroize::Zeroizing;

use crate::core::{
    ArtifactKind, Capability, Ecosystem, Operation, Phase, RiskShape, TaskKind, ToolFamily,
};

pub(crate) fn infer_risks(text: &str) -> BTreeSet<RiskShape> {
    TaskIntent::classify(text).risks
}

/// Controlled, content-free task hints derived from an ephemeral query.
/// The source query is never retained in this value.
#[derive(Default)]
pub(super) struct TaskIntent {
    capabilities: BTreeSet<Capability>,
    operations: BTreeSet<Operation>,
    tasks: BTreeSet<TaskKind>,
    ecosystems: BTreeSet<Ecosystem>,
    tool_families: BTreeSet<ToolFamily>,
    phases: BTreeSet<Phase>,
    artifacts: BTreeSet<ArtifactKind>,
    risks: BTreeSet<RiskShape>,
}

impl TaskIntent {
    pub fn classify(query: &str) -> Self {
        let normalized = Zeroizing::new(query.to_ascii_lowercase());
        let mut intent = Self::default();
        for token in normalized.split(|character: char| !character.is_ascii_alphanumeric()) {
            match token {
                "delete" | "deletion" | "remove" | "overwrite" | "prune" | "reset" => {
                    intent.risks.insert(RiskShape::Destructive);
                }
                "version" | "upgrade" | "migration" | "compatibility" | "schema" => {
                    intent.risks.insert(RiskShape::VersionSensitive);
                }
                "sandbox" | "apparmor" | "permission" | "keyring" => {
                    intent.risks.insert(RiskShape::SandboxSensitive);
                }
                "flaky" | "race" | "nondeterministic" | "intermittent" => {
                    intent.risks.insert(RiskShape::Flaky);
                }
                "expensive" | "large" | "benchmark" | "exhaustive" => {
                    intent.risks.insert(RiskShape::Expensive);
                }
                _ => {}
            }
            match token {
                "test" | "tests" | "testing" | "pytest" | "unittest" => {
                    intent.capabilities.insert(Capability::Test);
                    intent.operations.insert(Operation::Command);
                }
                "build" | "compile" | "compiler" | "link" => {
                    intent.capabilities.insert(Capability::Build);
                    intent.operations.insert(Operation::Command);
                }
                "release" | "package" | "packaging" => {
                    intent.capabilities.insert(Capability::Build);
                    intent.capabilities.insert(Capability::PackageManager);
                    intent.operations.insert(Operation::Command);
                }
                "lint" | "clippy" => {
                    intent.capabilities.insert(Capability::Lint);
                    intent.operations.insert(Operation::Command);
                }
                "format" | "fmt" | "rustfmt" => {
                    intent.capabilities.insert(Capability::Format);
                    intent.operations.insert(Operation::Command);
                }
                "edit" | "change" | "modify" | "fix" | "implement" | "refactor" | "patch" => {
                    intent.capabilities.insert(Capability::Edit);
                    intent.capabilities.insert(Capability::Patch);
                    intent.operations.insert(Operation::ApplyPatch);
                    intent.operations.insert(Operation::WriteFile);
                }
                "read" | "inspect" | "review" => {
                    intent.capabilities.insert(Capability::Filesystem);
                    intent.operations.insert(Operation::ReadFile);
                }
                "write" | "writing" | "atomic" | "config" | "configuration" => {
                    intent.capabilities.insert(Capability::Filesystem);
                    intent.operations.insert(Operation::WriteFile);
                }
                "search" | "find" | "grep" | "lookup" | "rg" => {
                    intent.capabilities.insert(Capability::Search);
                    intent.operations.insert(Operation::Search);
                }
                "git" | "commit" | "merge" | "rebase" | "branch" | "push" | "pull" => {
                    intent.capabilities.insert(Capability::Git);
                    intent.operations.insert(Operation::Command);
                }
                "web" | "http" | "https" | "network" | "download" | "fetch" => {
                    intent.capabilities.insert(Capability::Network);
                    intent.capabilities.insert(Capability::Browser);
                    intent.operations.insert(Operation::WebRequest);
                }
                "container" | "docker" | "podman" => {
                    intent.capabilities.insert(Capability::Container);
                    intent.operations.insert(Operation::Command);
                }
                "wait" | "poll" => {
                    intent.capabilities.insert(Capability::Wait);
                    intent.operations.insert(Operation::Wait);
                }
                "image" | "screenshot" | "vision" => {
                    intent.capabilities.insert(Capability::Vision);
                    intent.operations.insert(Operation::InspectImage);
                }
                "plan" | "planning" => {
                    intent.operations.insert(Operation::UpdatePlan);
                }
                "verify" | "verified" | "verification" => {
                    intent.capabilities.insert(Capability::Verify);
                    intent.capabilities.insert(Capability::Test);
                    intent.operations.insert(Operation::Command);
                }
                "hook" | "hooks" | "integration" | "native" | "transcript" => {
                    intent.capabilities.insert(Capability::Other);
                    intent.operations.insert(Operation::ToolCall);
                }
                "preview" | "apply" | "install" | "installation" => {
                    intent.capabilities.insert(Capability::Verify);
                    intent.capabilities.insert(Capability::PackageManager);
                    intent.operations.insert(Operation::ToolCall);
                }
                "research" | "analysis" | "analyze" | "reproduce" | "compare" | "subject"
                | "streaming" | "resource" | "budget" | "cap" => {
                    intent.capabilities.insert(Capability::Research);
                    intent.operations.insert(Operation::Analyze);
                }
                _ => {}
            }
            match token {
                "config" | "configuration" | "settings" | "setup" => {
                    intent.tasks.insert(TaskKind::Configuration);
                }
                "data" | "import" | "importer" | "ingest" | "ingestion" | "etl" | "dataset"
                | "csv" | "migration" => {
                    intent.tasks.insert(TaskKind::DataImport);
                }
                "bug" | "debug" | "debugging" | "error" | "failure" | "broken" | "fix" => {
                    intent.tasks.insert(TaskKind::Debugging);
                }
                "dependency" | "dependencies" | "upgrade" | "update" => {
                    intent.tasks.insert(TaskKind::DependencyUpdate);
                }
                "doc" | "docs" | "documentation" | "readme" => {
                    intent.tasks.insert(TaskKind::Documentation);
                }
                "feature" | "implement" | "implementation" | "build" | "create" | "add" => {
                    intent.tasks.insert(TaskKind::FeatureImplementation);
                }
                "integration" | "api" | "provider" | "service" | "database" => {
                    intent.tasks.insert(TaskKind::Integration);
                }
                "performance" | "optimize" | "optimization" | "latency" | "memory" => {
                    intent.tasks.insert(TaskKind::Performance);
                }
                "refactor" | "refactoring" | "cleanup" => {
                    intent.tasks.insert(TaskKind::Refactoring);
                }
                "release" | "package" | "packaging" | "publish" | "deploy" => {
                    intent.tasks.insert(TaskKind::Release);
                }
                "research" | "analysis" | "analyze" | "compare" | "investigate" => {
                    intent.tasks.insert(TaskKind::Research);
                }
                "security" | "privacy" | "credential" | "encryption" | "audit" => {
                    intent.tasks.insert(TaskKind::Security);
                }
                "test" | "tests" | "testing" | "verify" | "verification" | "pytest"
                | "unittest" => {
                    intent.tasks.insert(TaskKind::Testing);
                }
                _ => {}
            }
            match token {
                "rust" | "cargo" | "clippy" | "rustfmt" | "crate" => {
                    intent.ecosystems.insert(Ecosystem::Rust);
                    intent.tool_families.insert(ToolFamily::Cargo);
                }
                "python" | "pytest" | "pip" | "venv" | "uv" => {
                    intent.ecosystems.insert(Ecosystem::Python);
                    intent.tool_families.insert(ToolFamily::Pytest);
                }
                "javascript" | "node" | "npm" | "npx" => {
                    intent.ecosystems.insert(Ecosystem::Javascript);
                    intent.tool_families.insert(ToolFamily::Npm);
                }
                "typescript" | "tsc" => {
                    intent.ecosystems.insert(Ecosystem::Typescript);
                }
                "jest" | "vitest" => {
                    intent.tool_families.insert(ToolFamily::Jest);
                }
                "pnpm" => {
                    intent.tool_families.insert(ToolFamily::Pnpm);
                }
                "yarn" => {
                    intent.tool_families.insert(ToolFamily::Yarn);
                }
                "go" | "golang" => {
                    intent.ecosystems.insert(Ecosystem::Go);
                }
                "java" | "gradle" | "maven" => {
                    intent.ecosystems.insert(Ecosystem::Java);
                }
                "dotnet" | "csharp" | "nuget" => {
                    intent.ecosystems.insert(Ecosystem::Dotnet);
                }
                "cmake" => {
                    intent.ecosystems.insert(Ecosystem::Cpp);
                    intent.tool_families.insert(ToolFamily::Cmake);
                }
                "make" | "makefile" => {
                    intent.tool_families.insert(ToolFamily::Make);
                }
                "gcc" => {
                    intent.ecosystems.insert(Ecosystem::C);
                    intent.tool_families.insert(ToolFamily::Gcc);
                }
                "clang" => {
                    intent.ecosystems.insert(Ecosystem::Cpp);
                    intent.tool_families.insert(ToolFamily::Clang);
                }
                "cuda" | "nvcc" | "gpu" | "kernel" => {
                    intent.ecosystems.insert(Ecosystem::Cuda);
                    intent.tool_families.insert(ToolFamily::Nvcc);
                }
                "php" | "composer" => {
                    intent.ecosystems.insert(Ecosystem::Php);
                }
                "laravel" | "artisan" => {
                    intent.ecosystems.insert(Ecosystem::Laravel);
                }
                "ruby" | "rails" | "bundler" => {
                    intent.ecosystems.insert(Ecosystem::Ruby);
                }
                "shell" | "bash" | "zsh" | "script" => {
                    intent.ecosystems.insert(Ecosystem::Shell);
                }
                "kicad" | "pcb" | "schematic" | "footprint" | "drc" | "erc" | "gerber" => {
                    intent.ecosystems.insert(Ecosystem::Kicad);
                    intent.tool_families.insert(ToolFamily::Kicad);
                    intent.artifacts.insert(ArtifactKind::NativeCad);
                }
                "sql" | "sqlite" | "postgres" | "mysql" | "schema" => {
                    intent.ecosystems.insert(Ecosystem::Sql);
                }
                "kubectl" | "kubernetes" | "k8s" | "helm" => {
                    intent.tool_families.insert(ToolFamily::Kubectl);
                }
                "terraform" | "tofu" => {
                    intent.tool_families.insert(ToolFamily::Terraform);
                }
                "playwright" | "e2e" => {
                    intent.tool_families.insert(ToolFamily::Playwright);
                }
                _ => {}
            }
            match token {
                "git" | "commit" | "merge" | "rebase" | "branch" => {
                    intent.ecosystems.insert(Ecosystem::Git);
                    intent.tool_families.insert(ToolFamily::Git);
                }
                "docker" | "podman" | "container" | "dockerfile" | "compose" => {
                    intent.ecosystems.insert(Ecosystem::Docker);
                    intent.tool_families.insert(ToolFamily::Docker);
                }
                _ => {}
            }
            match token {
                "debug" | "debugging" | "diagnose" | "investigate" | "bug" | "failure"
                | "error" | "broken" | "flaky" => {
                    intent.phases.insert(Phase::Diagnose);
                }
                "edit" | "change" | "modify" | "fix" | "implement" | "refactor" | "patch"
                | "write" | "add" | "remove" | "rename" => {
                    intent.phases.insert(Phase::Mutate);
                }
                "test" | "tests" | "testing" | "verify" | "verification" | "check" | "lint"
                | "validate" => {
                    intent.phases.insert(Phase::Verify);
                }
                "release" | "publish" | "deploy" | "package" | "ship" | "tag" => {
                    intent.phases.insert(Phase::Release);
                }
                "research" | "analysis" | "analyze" | "compare" | "explore" | "survey" => {
                    intent.phases.insert(Phase::Research);
                }
                "integration" | "integrate" | "hook" | "hooks" | "plugin" | "adapter" | "api" => {
                    intent.phases.insert(Phase::Integration);
                }
                _ => {}
            }
            match token {
                "generated" | "codegen" | "autogenerated" | "scaffold" => {
                    intent.artifacts.insert(ArtifactKind::GeneratedSource);
                }
                "config" | "configuration" | "settings" | "toml" | "yaml" | "json" | "ini"
                | "dotenv" => {
                    intent.artifacts.insert(ArtifactKind::Config);
                }
                "dataset" | "csv" | "parquet" | "data" => {
                    intent.artifacts.insert(ArtifactKind::Dataset);
                }
                "binary" | "executable" | "artifact" | "wheel" | "tarball" => {
                    intent.artifacts.insert(ArtifactKind::Binary);
                }
                "doc" | "docs" | "documentation" | "readme" | "changelog" => {
                    intent.artifacts.insert(ArtifactKind::Docs);
                }
                "source" | "code" | "module" | "function" | "struct" | "class" => {
                    intent.artifacts.insert(ArtifactKind::Source);
                }
                _ => {}
            }
        }
        intent
    }

    pub fn has_task_hints(&self) -> bool {
        !self.tasks.is_empty()
    }

    pub fn ecosystems(&self) -> &BTreeSet<Ecosystem> {
        &self.ecosystems
    }

    pub fn tool_families(&self) -> &BTreeSet<ToolFamily> {
        &self.tool_families
    }

    pub fn phases(&self) -> &BTreeSet<Phase> {
        &self.phases
    }

    pub fn artifacts(&self) -> &BTreeSet<ArtifactKind> {
        &self.artifacts
    }

    pub fn risks(&self) -> &BTreeSet<RiskShape> {
        &self.risks
    }

    pub fn matches_task(&self, task: TaskKind) -> bool {
        self.tasks.contains(&task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_task_text_to_controlled_hints() {
        let intent = TaskIntent::classify("Fix the SECRET pytest failure with a patch");
        assert!(intent.capabilities.contains(&Capability::Test));
        assert!(intent.capabilities.contains(&Capability::Patch));
        assert!(intent.operations.contains(&Operation::Command));
        assert!(intent.operations.contains(&Operation::ApplyPatch));
    }

    #[test]
    fn recognizes_controlled_practice_categories() {
        let intent = TaskIntent::classify(
            "atomic config write, targeted verification, native hook integration release",
        );
        assert!(intent.capabilities.contains(&Capability::Filesystem));
        assert!(intent.capabilities.contains(&Capability::Test));
        assert!(intent.capabilities.contains(&Capability::Other));
        assert!(intent.capabilities.contains(&Capability::Build));
        assert!(intent.operations.contains(&Operation::WriteFile));
        assert!(intent.operations.contains(&Operation::ToolCall));
        assert!(intent.operations.contains(&Operation::Command));
    }

    #[test]
    fn matches_tokens_not_substrings() {
        let intent = TaskIntent::classify("contestant dispatches work");
        assert!(!intent.capabilities.contains(&Capability::Test));
        assert!(!intent.operations.contains(&Operation::ApplyPatch));
        assert!(!intent.has_task_hints());
    }

    #[test]
    fn recognizes_research_strategy_terms_without_retaining_the_query() {
        let intent = TaskIntent::classify(
            "reproduce then compare with per-subject streaming and a resource cap",
        );

        assert!(intent.capabilities.contains(&Capability::Research));
        assert!(intent.operations.contains(&Operation::Analyze));
        assert!(intent.matches_task(TaskKind::Research));
    }

    #[test]
    fn infers_controlled_applicability_hints() {
        let intent = TaskIntent::classify("debug generated kicad pcb placement drc failure");
        assert!(intent.ecosystems().contains(&Ecosystem::Kicad));
        assert!(intent.tool_families().contains(&ToolFamily::Kicad));
        assert!(intent.artifacts().contains(&ArtifactKind::NativeCad));
        assert!(intent.artifacts().contains(&ArtifactKind::GeneratedSource));
        assert!(intent.phases().contains(&Phase::Diagnose));
        assert!(TaskIntent::classify("zzz").ecosystems().is_empty());
    }

    #[test]
    fn infers_only_controlled_risk_shapes() {
        let intent = TaskIntent::classify(
            "prune a version migration inside the sandbox after an intermittent benchmark",
        );
        assert!(intent.risks().contains(&RiskShape::Destructive));
        assert!(intent.risks().contains(&RiskShape::VersionSensitive));
        assert!(intent.risks().contains(&RiskShape::SandboxSensitive));
        assert!(intent.risks().contains(&RiskShape::Flaky));
        assert!(intent.risks().contains(&RiskShape::Expensive));
        assert!(
            TaskIntent::classify("ordinary feature work")
                .risks()
                .is_empty()
        );
    }

    #[test]
    fn discriminates_data_import_from_unrecognized_text() {
        let import = TaskIntent::classify("build a data importer");
        let unrelated = TaskIntent::classify("zzz nonsense unrelated");

        assert!(import.matches_task(TaskKind::DataImport));
        assert!(!unrelated.matches_task(TaskKind::DataImport));
    }
}
