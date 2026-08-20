use anyhow::{Result, anyhow, bail};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, Generate, KeyInit, Payload},
};
use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::{EvidenceKind, HistoryEvent};

use super::MasterKey;

pub const ENVELOPE_VERSION: u32 = 1;
const EVENT_KEY_INFO: &[u8] = b"regurgitate:event-encryption:v1";
const AAD_DOMAIN: &[u8] = b"regurgitate:event-envelope";
const NONCE_BYTES: usize = 24;

pub struct EventCipher {
    cipher: XChaCha20Poly1305,
}

pub struct SealedEvent {
    pub nonce: [u8; NONCE_BYTES],
    pub ciphertext: Vec<u8>,
}

pub struct EnvelopeMetadata {
    pub event_id: Uuid,
    pub created_at_ms: i64,
    pub evidence_kind: EvidenceKind,
    pub schema_version: u32,
    pub envelope_version: u32,
}

impl EnvelopeMetadata {
    pub fn for_event(event: &HistoryEvent) -> Self {
        Self {
            event_id: event.id,
            created_at_ms: event.timestamp.timestamp_millis(),
            evidence_kind: event.evidence_kind,
            schema_version: event.schema_version,
            envelope_version: ENVELOPE_VERSION,
        }
    }

    fn associated_data(&self) -> Vec<u8> {
        let mut aad = Vec::with_capacity(AAD_DOMAIN.len() + 4 + 16 + 8 + 1 + 4);
        aad.extend_from_slice(AAD_DOMAIN);
        aad.extend_from_slice(&self.envelope_version.to_be_bytes());
        aad.extend_from_slice(self.event_id.as_bytes());
        aad.extend_from_slice(&self.created_at_ms.to_be_bytes());
        aad.push(self.evidence_kind.storage_code());
        aad.extend_from_slice(&self.schema_version.to_be_bytes());
        aad
    }
}

impl EventCipher {
    pub fn new(master_key: &MasterKey) -> Result<Self> {
        let hkdf = Hkdf::<Sha256>::new(None, master_key.as_bytes());
        let mut event_key = Zeroizing::new([0_u8; 32]);
        hkdf.expand(EVENT_KEY_INFO, event_key.as_mut())
            .map_err(|_| anyhow!("could not derive the Regurgitate event encryption key"))?;
        let cipher = XChaCha20Poly1305::new_from_slice(event_key.as_ref())
            .map_err(|_| anyhow!("derived Regurgitate event key has an invalid length"))?;
        Ok(Self { cipher })
    }

    pub fn seal(&self, event: &HistoryEvent) -> Result<SealedEvent> {
        let metadata = EnvelopeMetadata::for_event(event);
        let mut plaintext = Zeroizing::new(Vec::new());
        ciborium::into_writer(event, &mut *plaintext)
            .map_err(|error| anyhow!("could not serialize event as CBOR: {error}"))?;

        let nonce = XNonce::generate();
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: &metadata.associated_data(),
                },
            )
            .map_err(|_| anyhow!("could not encrypt Regurgitate event"))?;

        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        nonce_bytes.copy_from_slice(&nonce);
        Ok(SealedEvent {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn open(
        &self,
        metadata: &EnvelopeMetadata,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<HistoryEvent> {
        if metadata.envelope_version != ENVELOPE_VERSION {
            bail!(
                "unsupported Regurgitate encryption envelope version {}",
                metadata.envelope_version
            );
        }
        let nonce = XNonce::try_from(nonce).map_err(|_| anyhow!("invalid event nonce length"))?;
        let plaintext = self
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: &metadata.associated_data(),
                },
            )
            .map_err(|_| anyhow!("event authentication failed"))?;
        let plaintext = Zeroizing::new(plaintext);
        let event: HistoryEvent = ciborium::from_reader(plaintext.as_slice())
            .map_err(|error| anyhow!("could not deserialize encrypted event: {error}"))?;

        if event.id != metadata.event_id
            || event.evidence_kind != metadata.evidence_kind
            || event.schema_version != metadata.schema_version
            || event.timestamp.timestamp_millis() != metadata.created_at_ms
        {
            bail!("encrypted event metadata does not match its authenticated envelope");
        }
        Ok(event)
    }
}
