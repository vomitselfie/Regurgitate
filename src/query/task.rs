use std::collections::BTreeSet;

use zeroize::Zeroizing;

use crate::core::{Capability, Operation};

/// Controlled, content-free task hints derived from an ephemeral query.
/// The source query is never retained in this value.
#[derive(Default)]
pub(super) struct TaskIntent {
    capabilities: BTreeSet<Capability>,
    operations: BTreeSet<Operation>,
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
                _ => {}
            }
        }
        intent
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
    fn matches_tokens_not_substrings() {
        let intent = TaskIntent::classify("contestant dispatches work");
        assert_eq!(intent.relevance(Capability::Test, Operation::ApplyPatch), 0);
    }
}
