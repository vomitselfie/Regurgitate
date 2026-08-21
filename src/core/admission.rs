//! Structural admission checks for bounded capsule text.
//!
//! These checks are deliberately conservative. They exist to keep prompts,
//! source, commands, URLs, paths, secrets, and provider payloads out of the
//! semantic vault. A false rejection costs one rephrased sentence; a false
//! acceptance would store material the privacy contract forbids. When in
//! doubt, reject.

use std::fmt;

/// Minimum number of words for a lesson-like sentence. A one-word "lesson"
/// is either useless or an identifier.
const MIN_WORDS: usize = 3;
/// Any single token at or above this length is treated as opaque material.
const MAX_TOKEN_CHARS: usize = 40;
/// Tokens at or above this length that also carry a digit look like hashes,
/// keys, or identifiers rather than words.
const MAX_DIGIT_TOKEN_CHARS: usize = 24;
/// Upper bound on the fraction of punctuation-like characters before the text
/// is treated as code or structured data.
const MAX_SYMBOL_RATIO: f64 = 0.15;

/// Characters that are far more common in commands, code, templates, and
/// serialized payloads than in a notebook sentence.
const FORBIDDEN_CHARS: &[char] = &[
    '`', '$', '|', '{', '}', '<', '>', '\\', '=', '#', '@', '^', '~', '*',
];

/// Lower-case markers that indicate credentials or key material.
const SECRET_MARKERS: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "akia",
    "xoxb",
    "xoxp",
    "-----begin",
    "password",
    "passwd",
    "secret",
    "token:",
    "token=",
    "access token",
    "auth token",
    "session token",
    "api key",
    "apikey",
    "api_key",
    "bearer",
    "credential",
    "private key",
];

/// Conversational words that mark transcript-like material rather than an
/// impersonal notebook sentence. Lessons are written in the imperative.
const TRANSCRIPT_WORDS: &[&str] = &[
    "i",
    "i'm",
    "i'll",
    "i've",
    "i'd",
    "me",
    "my",
    "we",
    "we'll",
    "we've",
    "you",
    "you'll",
    "you're",
    "your",
    "please",
    "thanks",
    "thank",
    "hello",
    "hi",
    "user:",
    "assistant:",
    "system:",
];
const TRANSCRIPT_PHRASES: &[&str] = &[
    "the user",
    "the assistant",
    "the prompt",
    "the model",
    "the agent said",
    "as requested",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionRejection {
    Empty,
    TooShort,
    TooLong,
    ControlCharacter,
    TranscriptLike,
    UrlLike,
    PathLike,
    CommandOrCodeLike,
    SecretLike,
    OpaqueToken,
}

impl fmt::Display for AdmissionRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::Empty => "text is empty",
            Self::TooShort => "text must contain at least three words",
            Self::TooLong => "text exceeds its bounded length",
            Self::ControlCharacter => "text contains a line break or control character",
            Self::TranscriptLike => "text reads like a prompt or reply rather than a lesson",
            Self::UrlLike => "text resembles a URL",
            Self::PathLike => "text resembles a filesystem path",
            Self::CommandOrCodeLike => "text resembles a command, code, or structured payload",
            Self::SecretLike => "text resembles credential material",
            Self::OpaqueToken => "text contains an opaque identifier-like token",
        };
        formatter.write_str(reason)
    }
}

impl std::error::Error for AdmissionRejection {}

/// Validates one bounded, deliberately authored sentence.
pub fn admit_text(text: &str, max_chars: usize) -> Result<(), AdmissionRejection> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AdmissionRejection::Empty);
    }
    if trimmed.chars().count() > max_chars {
        return Err(AdmissionRejection::TooLong);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AdmissionRejection::ControlCharacter);
    }
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.contains("://") || lowered.contains("www.") || lowered.contains("localhost") {
        return Err(AdmissionRejection::UrlLike);
    }
    if looks_like_path(trimmed) {
        return Err(AdmissionRejection::PathLike);
    }
    if trimmed
        .chars()
        .any(|character| FORBIDDEN_CHARS.contains(&character))
    {
        return Err(AdmissionRejection::CommandOrCodeLike);
    }
    if SECRET_MARKERS.iter().any(|marker| lowered.contains(marker)) {
        return Err(AdmissionRejection::SecretLike);
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() < MIN_WORDS {
        return Err(AdmissionRejection::TooShort);
    }
    if TRANSCRIPT_PHRASES
        .iter()
        .any(|phrase| lowered.contains(phrase))
        || lowered
            .split_whitespace()
            .map(|word| word.trim_end_matches(|character: char| ",.!?;".contains(character)))
            .any(|word| TRANSCRIPT_WORDS.contains(&word))
    {
        return Err(AdmissionRejection::TranscriptLike);
    }
    for word in &words {
        let length = word.chars().count();
        let has_digit = word.chars().any(|character| character.is_ascii_digit());
        if length >= MAX_TOKEN_CHARS || (has_digit && length >= MAX_DIGIT_TOKEN_CHARS) {
            return Err(AdmissionRejection::OpaqueToken);
        }
    }
    let symbols = trimmed
        .chars()
        .filter(|character| {
            !character.is_alphanumeric()
                && !character.is_whitespace()
                && !",.'-:()/!?\"".contains(*character)
        })
        .count();
    let total = trimmed
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    if total > 0 && symbols as f64 / total as f64 > MAX_SYMBOL_RATIO {
        return Err(AdmissionRejection::CommandOrCodeLike);
    }
    Ok(())
}

fn looks_like_path(text: &str) -> bool {
    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|character: char| ",.;:()\"'".contains(character));
        if trimmed.starts_with('/')
            || trimmed.starts_with("~/")
            || trimmed.starts_with("./")
            || trimmed.starts_with("../")
            || trimmed.contains(":\\")
            || trimmed.contains("\\\\")
        {
            return true;
        }
        // "a/b" inside prose ("read/write") is fine; two or more separators
        // ("src/core/event.rs") is a path.
        if trimmed.matches('/').count() >= 2 {
            return true;
        }
        // A single separator plus a file extension ("core/event.rs").
        if trimmed.contains('/') && has_file_extension(trimmed) {
            return true;
        }
        if !trimmed.contains('/') && has_source_extension(trimmed) {
            return true;
        }
    }
    false
}

fn has_file_extension(word: &str) -> bool {
    word.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (1..=5).contains(&extension.len())
            && extension
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    })
}

fn has_source_extension(word: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        ".rs",
        ".py",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".go",
        ".java",
        ".kt",
        ".c",
        ".h",
        ".cpp",
        ".hpp",
        ".cs",
        ".rb",
        ".php",
        ".sh",
        ".toml",
        ".yaml",
        ".yml",
        ".json",
        ".sql",
        ".env",
        ".pem",
        ".key",
        ".db",
        ".sqlite",
        ".lock",
        ".cfg",
        ".ini",
        ".kicad_pcb",
        ".kicad_sch",
    ];
    let lowered = word.to_ascii_lowercase();
    EXTENSIONS
        .iter()
        .any(|extension| lowered.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_notebook_sentences() {
        for text in [
            "Generated native artifact; parser acceptance is weaker than native verification.",
            "Change one placement class at a time and run native verification before continuing.",
            "Do not infer correctness from serialization success alone.",
            "Preview destructive config edits before applying atomically.",
            "Read/write races appear only under the full suite, not targeted runs.",
        ] {
            assert_eq!(admit_text(text, 320), Ok(()), "{text:?}");
        }
    }

    #[test]
    fn rejects_urls_paths_and_files() {
        assert_eq!(
            admit_text("See https://private.example.test/wiki for details", 320),
            Err(AdmissionRejection::UrlLike)
        );
        assert_eq!(
            admit_text(
                "The bug lives in /home/alice/project/src/main.rs today",
                320
            ),
            Err(AdmissionRejection::PathLike)
        );
        assert_eq!(
            admit_text("Edit core/event.rs before the release", 320),
            Err(AdmissionRejection::PathLike)
        );
        assert_eq!(
            admit_text("Edit the file event.rs before the release", 320),
            Err(AdmissionRejection::PathLike)
        );
        assert_eq!(
            admit_text("Check C:\\Users\\alice\\secrets first", 320),
            Err(AdmissionRejection::PathLike)
        );
    }

    #[test]
    fn rejects_commands_code_and_payloads() {
        assert_eq!(
            admit_text("Run `cargo test -- --nocapture` to verify", 320),
            Err(AdmissionRejection::CommandOrCodeLike)
        );
        assert_eq!(
            admit_text("curl -s $HOST | jq .data", 320),
            Err(AdmissionRejection::CommandOrCodeLike)
        );
        assert_eq!(
            admit_text("{\"tool\": \"shell\", \"args\": [\"rm\"]}", 320),
            Err(AdmissionRejection::CommandOrCodeLike)
        );
        assert_eq!(
            admit_text("fn main() -> Result<()> works fine", 320),
            Err(AdmissionRejection::CommandOrCodeLike)
        );
        assert_eq!(
            admit_text("set DEBUG=1 before running tests", 320),
            Err(AdmissionRejection::CommandOrCodeLike)
        );
    }

    #[test]
    fn rejects_secrets_and_opaque_tokens() {
        assert_eq!(
            admit_text("the key is sk-fixture-secret-value for staging", 320),
            Err(AdmissionRejection::SecretLike)
        );
        assert_eq!(
            admit_text("use password hunter2 when prompted", 320),
            Err(AdmissionRejection::SecretLike)
        );
        assert_eq!(
            admit_text(
                "commit 3f9a8c1d2e4b5a6f7c8d9e0f1a2b3c4d5e6f7a8b fixed it",
                320
            ),
            Err(AdmissionRejection::OpaqueToken)
        );
        assert_eq!(
            admit_text("the AKIAIOSFODNN7EXAMPLE account was used", 320),
            Err(AdmissionRejection::SecretLike)
        );
    }

    #[test]
    fn rejects_conversational_text() {
        assert_eq!(
            admit_text("I will now run the tests and report back.", 320),
            Err(AdmissionRejection::TranscriptLike)
        );
        assert_eq!(
            admit_text("Please refactor the auth module as the user asked", 320),
            Err(AdmissionRejection::TranscriptLike)
        );
        assert_eq!(
            admit_text("Run the native check before trusting the parser.", 320),
            Ok(())
        );
    }

    #[test]
    fn rejects_empty_short_long_and_multiline_text() {
        assert_eq!(admit_text("   ", 320), Err(AdmissionRejection::Empty));
        assert_eq!(
            admit_text("verify first", 320),
            Err(AdmissionRejection::TooShort)
        );
        assert_eq!(
            admit_text("one two three four five", 10),
            Err(AdmissionRejection::TooLong)
        );
        assert_eq!(
            admit_text("first line\nsecond line of a transcript", 320),
            Err(AdmissionRejection::ControlCharacter)
        );
    }
}
