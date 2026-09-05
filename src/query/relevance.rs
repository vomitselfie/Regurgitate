//! Hard exclusions for known incompatible domains. Ranking still handles
//! missing metadata, project defaults, and degrees of contextual agreement.

use crate::core::{Ecosystem, ToolFamily};

/// Closely related language ecosystems remain eligible; they still receive
/// distinct ranking scores. Generic tags do not assert a language restriction.
fn compatible_ecosystems(left: Ecosystem, right: Ecosystem) -> bool {
    use Ecosystem::*;
    left == right
        || matches!(left, Generic)
        || matches!(right, Generic)
        || matches!(
            (left, right),
            (Javascript, Typescript)
                | (Typescript, Javascript)
                | (Php, Laravel)
                | (Laravel, Php)
                | (C, Cpp)
                | (Cpp, C)
                | (Cuda, C | Cpp)
                | (C | Cpp, Cuda)
        )
}

pub(super) fn ecosystem_conflicts(
    explicit: Option<Ecosystem>,
    inferred: impl Iterator<Item = Ecosystem>,
    recorded: Option<Ecosystem>,
) -> bool {
    let Some(recorded) = recorded else {
        return false;
    };
    if let Some(explicit) = explicit {
        return !compatible_ecosystems(explicit, recorded);
    }
    let mut inferred = inferred.peekable();
    inferred.peek().is_some() && !inferred.any(|hint| compatible_ecosystems(hint, recorded))
}

/// Only an explicit tool selection is a hard exclusion. Query mentions can
/// name cooperating tools; a language name does not select its test runner.
pub(super) fn tool_conflicts(explicit: Option<ToolFamily>, recorded: Option<ToolFamily>) -> bool {
    match (explicit, recorded) {
        (Some(ToolFamily::Other), _) | (_, Some(ToolFamily::Other)) => false,
        (Some(wanted), Some(recorded)) => wanted != recorded,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_conflicts_preserve_unknown_generic_and_related_domains() {
        use Ecosystem::*;
        assert!(ecosystem_conflicts(None, [Python].into_iter(), Some(Rust)));
        assert!(!ecosystem_conflicts(
            Some(Rust),
            [Python].into_iter(),
            Some(Rust)
        ));
        for recorded in [None, Some(Generic), Some(Python)] {
            assert!(!ecosystem_conflicts(None, [Python].into_iter(), recorded));
        }
        for (wanted, recorded) in [
            (Javascript, Typescript),
            (Php, Laravel),
            (Cpp, C),
            (Cuda, Cpp),
        ] {
            assert!(!ecosystem_conflicts(
                None,
                [wanted].into_iter(),
                Some(recorded)
            ));
        }
        assert!(!ecosystem_conflicts(
            None,
            [Rust, Python].into_iter(),
            Some(Python)
        ));
        assert!(!ecosystem_conflicts(None, [].into_iter(), Some(Rust)));
        assert!(!tool_conflicts(Some(ToolFamily::Pytest), None));
        assert!(!tool_conflicts(
            Some(ToolFamily::Other),
            Some(ToolFamily::Cargo)
        ));
        assert!(tool_conflicts(
            Some(ToolFamily::Yarn),
            Some(ToolFamily::Pnpm)
        ));
    }
}
