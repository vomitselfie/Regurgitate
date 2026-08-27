//! Versioned encrypted experience payload codec.
//!
//! Database rows retain the schema version authenticated by their envelope.
//! Decoding upgrades older payloads in memory; reads never rewrite storage.

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{
    ApplicabilityTags, Caveat, EXPERIENCE_SCHEMA_VERSION, EnvironmentFingerprint, EvidenceEntry,
    ExperienceCapsule, Lesson, MemoryLifecycle, MemoryScope, Procedure, Situation, TaskKind,
};

pub(super) const EXPERIENCE_SCHEMA_V3: u32 = 3;

pub(super) struct DecodedExperience {
    pub capsule: ExperienceCapsule,
    pub source_schema_version: u32,
}

pub(super) fn encode(capsule: &ExperienceCapsule, writer: &mut Vec<u8>) -> Result<()> {
    if capsule.schema_version != EXPERIENCE_SCHEMA_VERSION {
        bail!(
            "cannot encode experience schema {}; current schema is {}",
            capsule.schema_version,
            EXPERIENCE_SCHEMA_VERSION
        );
    }
    ciborium::into_writer(capsule, writer)
        .map_err(|error| anyhow!("could not serialize experience as CBOR: {error}"))
}

pub(super) fn decode(schema_version: u32, bytes: &[u8]) -> Result<DecodedExperience> {
    let capsule = match schema_version {
        EXPERIENCE_SCHEMA_V3 => {
            let legacy: ExperienceCapsuleV3 = ciborium::from_reader(bytes).map_err(|error| {
                anyhow!("could not deserialize encrypted v3 experience: {error}")
            })?;
            legacy.upgrade()
        }
        EXPERIENCE_SCHEMA_VERSION => ciborium::from_reader(bytes)
            .map_err(|error| anyhow!("could not deserialize encrypted experience: {error}"))?,
        other => bail!("unsupported experience schema version {other}"),
    };
    capsule.validate()?;
    Ok(DecodedExperience {
        capsule,
        source_schema_version: schema_version,
    })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceEntryV3 {
    at: DateTime<Utc>,
    outcome: crate::core::SemanticOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failure_reason: Option<crate::core::FailureReason>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExperienceCapsuleV3 {
    id: Uuid,
    project_id: Uuid,
    scope: MemoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope_id: Option<Uuid>,
    task: TaskKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    situation: Option<Situation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lesson: Option<Lesson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caveat: Option<Caveat>,
    procedure: Procedure,
    #[serde(default)]
    applicability: ApplicabilityTags,
    #[serde(default)]
    environment: EnvironmentFingerprint,
    lifecycle: MemoryLifecycle,
    evidence: Vec<EvidenceEntryV3>,
    created_at: DateTime<Utc>,
    last_confirmed_at: DateTime<Utc>,
    schema_version: u32,
}

impl ExperienceCapsuleV3 {
    fn upgrade(self) -> ExperienceCapsule {
        let mut capsule = ExperienceCapsule {
            id: self.id,
            project_id: self.project_id,
            scope: self.scope,
            scope_id: self.scope_id,
            task: self.task,
            situation: self.situation,
            lesson: self.lesson,
            caveat: self.caveat,
            procedure: self.procedure,
            applicability: self.applicability,
            lifecycle: self.lifecycle,
            challenge: None,
            evidence: self
                .evidence
                .into_iter()
                .map(|entry| {
                    EvidenceEntry::agent_reported(
                        entry.at,
                        entry.outcome,
                        entry.failure_reason,
                        self.environment,
                    )
                })
                .collect(),
            created_at: self.created_at,
            last_confirmed_at: self.last_confirmed_at,
            schema_version: EXPERIENCE_SCHEMA_VERSION,
        };
        if capsule.lifecycle == MemoryLifecycle::Challenged {
            capsule.challenge(capsule.last_confirmed_at);
        }
        capsule
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::core::{HostClass, SemanticOutcome, Strategy, ToolFamily};

    use super::*;

    #[test]
    fn upgrades_v3_environment_onto_each_evidence_entry() {
        let at = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let legacy = ExperienceCapsuleV3 {
            id: Uuid::from_u128(1),
            project_id: Uuid::from_u128(2),
            scope: MemoryScope::Project,
            scope_id: Some(Uuid::from_u128(2)),
            task: TaskKind::FeatureImplementation,
            situation: None,
            lesson: None,
            caveat: None,
            procedure: Procedure::from_strategy(Strategy::TargetedVerification),
            applicability: Default::default(),
            environment: EnvironmentFingerprint {
                tool_family: Some(ToolFamily::Cargo),
                major_version: Some(1),
                host_class: Some(HostClass::Linux),
            },
            lifecycle: MemoryLifecycle::Active,
            evidence: vec![EvidenceEntryV3 {
                at,
                outcome: SemanticOutcome::Success,
                failure_reason: None,
            }],
            created_at: at,
            last_confirmed_at: at,
            schema_version: EXPERIENCE_SCHEMA_V3,
        };
        let mut bytes = Vec::new();
        ciborium::into_writer(&legacy, &mut bytes).unwrap();

        let decoded = decode(EXPERIENCE_SCHEMA_V3, &bytes).unwrap();

        assert_eq!(decoded.source_schema_version, EXPERIENCE_SCHEMA_V3);
        assert_eq!(decoded.capsule.schema_version, EXPERIENCE_SCHEMA_VERSION);
        assert_eq!(decoded.capsule.evidence[0].environment, legacy.environment);
    }

    #[test]
    fn encrypted_v3_envelope_opens_as_v4_without_changing_envelope_version() {
        use crate::storage::{
            MasterKey,
            crypto::{ENVELOPE_VERSION, ExperienceCipher, ExperienceEnvelope},
        };

        let at = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let legacy = ExperienceCapsuleV3 {
            id: Uuid::from_u128(11),
            project_id: Uuid::from_u128(12),
            scope: MemoryScope::Project,
            scope_id: Some(Uuid::from_u128(12)),
            task: TaskKind::Testing,
            situation: None,
            lesson: None,
            caveat: None,
            procedure: Procedure::from_strategy(Strategy::TargetedVerification),
            applicability: Default::default(),
            environment: EnvironmentFingerprint::default(),
            lifecycle: MemoryLifecycle::Active,
            evidence: vec![EvidenceEntryV3 {
                at,
                outcome: SemanticOutcome::Success,
                failure_reason: None,
            }],
            created_at: at,
            last_confirmed_at: at,
            schema_version: EXPERIENCE_SCHEMA_V3,
        };
        let mut plaintext = Vec::new();
        ciborium::into_writer(&legacy, &mut plaintext).unwrap();
        let envelope = ExperienceEnvelope {
            id: legacy.id,
            scope_token: [1; 32],
            origin_token: [2; 32],
            created_at_ms: at.timestamp_millis(),
            updated_at_ms: at.timestamp_millis(),
            schema_version: EXPERIENCE_SCHEMA_V3,
            envelope_version: ENVELOPE_VERSION,
        };
        let cipher = ExperienceCipher::new(&MasterKey::from_bytes([92; 32])).unwrap();
        let sealed = cipher
            .seal_fixture_plaintext(&envelope, &plaintext)
            .unwrap();

        let upgraded = cipher
            .open(&envelope, &sealed.nonce, &sealed.ciphertext)
            .unwrap();
        assert_eq!(upgraded.schema_version, EXPERIENCE_SCHEMA_VERSION);
        assert_eq!(envelope.envelope_version, ENVELOPE_VERSION);
    }
}
