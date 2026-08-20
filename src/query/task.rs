use std::collections::BTreeSet;

use zeroize::Zeroizing;

use crate::core::{Capability, Operation, TaskKind};

/// Controlled, content-free task hints derived from an ephemeral query.
/// The source query is never retained in this value.
#[derive(Default)]
pub(super) struct TaskIntent {
    capabilities: BTreeSet<Capability>,
    operations: BTreeSet<Operation>,
    tasks: BTreeSet<TaskKind>,
}

impl TaskIntent {
    pub fn classify(query: &str) -> Self {
        let normalized = Zeroizing::new(query.to_ascii_lowercase());
        let mut intent = Self::default();
        for token in normalized.split(|character: char| !character.is_ascii_alphanumeric()) {
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
        }
        intent
    }

    pub fn matches_task(&self, task: TaskKind) -> bool {
        self.tasks.contains(&task)
    }

    pub fn relevance(&self, capability: Capability, operation: Operation) -> usize {
        usize::from(self.operations.contains(&operation)) * 4
            + usize::from(self.capabilities.contains(&capability)) * 3
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
        assert_eq!(intent.relevance(Capability::Test, Operation::ApplyPatch), 0);
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
    fn discriminates_data_import_from_unrecognized_text() {
        let import = TaskIntent::classify("build a data importer");
        let unrelated = TaskIntent::classify("zzz nonsense unrelated");

        assert!(import.matches_task(TaskKind::DataImport));
        assert!(!unrelated.matches_task(TaskKind::DataImport));
    }
}
