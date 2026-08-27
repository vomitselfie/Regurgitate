//! Stateless authenticated references for agent-facing confirmation.

use anyhow::{Result, bail};
use uuid::Uuid;

use crate::application::ConfirmationReference;

use super::{private::PrivateRecordKind, sqlite::EncryptedStore};

const PREFIX: &str = "r1_";
const VERSION: u8 = 1;
const NONCE_BYTES: usize = 8;
const SIGNATURE_BYTES: usize = 12;
const PAYLOAD_BYTES: usize = 1 + 16 + NONCE_BYTES;
const TOKEN_BYTES: usize = PAYLOAD_BYTES + SIGNATURE_BYTES;

impl EncryptedStore {
    pub(super) fn issue_confirmation_reference(&self, capsule_id: Uuid) -> Result<String> {
        let mut token = [0_u8; TOKEN_BYTES];
        token[0] = VERSION;
        token[1..17].copy_from_slice(capsule_id.as_bytes());
        getrandom::fill(&mut token[17..PAYLOAD_BYTES])?;
        let signature = self.private_cipher.lookup_token(
            PrivateRecordKind::ConfirmationReference,
            &token[..PAYLOAD_BYTES],
        )?;
        token[PAYLOAD_BYTES..].copy_from_slice(&signature[..SIGNATURE_BYTES]);
        Ok(format!("{PREFIX}{}", encode_base64url(&token)))
    }

    pub(super) fn resolve_confirmation_reference(
        &self,
        reference: &str,
    ) -> Result<Option<ConfirmationReference>> {
        let Some(encoded) = reference.trim().strip_prefix(PREFIX) else {
            return Ok(None);
        };
        let token = decode_base64url::<TOKEN_BYTES>(encoded)
            .ok_or_else(|| anyhow::anyhow!("invalid confirmation reference"))?;
        if token[0] != VERSION {
            bail!("invalid confirmation reference");
        }
        let expected = self.private_cipher.lookup_token(
            PrivateRecordKind::ConfirmationReference,
            &token[..PAYLOAD_BYTES],
        )?;
        if !constant_time_eq(&token[PAYLOAD_BYTES..], &expected[..SIGNATURE_BYTES]) {
            bail!("invalid confirmation reference");
        }
        let capsule_id = Uuid::from_slice(&token[1..17])
            .map_err(|_| anyhow::anyhow!("invalid confirmation reference"))?;
        let mut digest_input = Vec::with_capacity(7 + TOKEN_BYTES);
        digest_input.extend_from_slice(b"receipt");
        digest_input.extend_from_slice(&token);
        let receipt_digest = self
            .private_cipher
            .lookup_token(PrivateRecordKind::ConfirmationReference, &digest_input)?;
        Ok(Some(ConfirmationReference {
            capsule_id,
            receipt_digest,
        }))
    }
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_base64url(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(BASE64URL[((bits >> 18) & 63) as usize] as char);
        encoded.push(BASE64URL[((bits >> 12) & 63) as usize] as char);
        if chunk.len() >= 2 {
            encoded.push(BASE64URL[((bits >> 6) & 63) as usize] as char);
        }
        if chunk.len() == 3 {
            encoded.push(BASE64URL[(bits & 63) as usize] as char);
        }
    }
    encoded
}

fn decode_base64url<const N: usize>(encoded: &str) -> Option<[u8; N]> {
    if !encoded.is_ascii() || encoded.len() % 4 == 1 {
        return None;
    }
    let mut decoded = Vec::with_capacity(N);
    let mut buffer = 0_u32;
    let mut buffered_bits = 0_u8;
    for character in encoded.bytes() {
        let value = BASE64URL.iter().position(|known| *known == character)? as u32;
        buffer = (buffer << 6) | value;
        buffered_bits += 6;
        if buffered_bits >= 8 {
            buffered_bits -= 8;
            decoded.push(((buffer >> buffered_bits) & 0xff) as u8);
            buffer &= (1_u32 << buffered_bits).wrapping_sub(1);
        }
    }
    if buffer != 0 || decoded.len() != N || encode_base64url(&decoded) != encoded {
        return None;
    }
    decoded.try_into().ok()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MasterKey;

    #[test]
    fn references_round_trip_and_reject_tampering() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([91; 32])).unwrap();
        let id = Uuid::from_u128(42);
        let reference = store.issue_confirmation_reference(id).unwrap();
        assert!(reference.starts_with(PREFIX));
        let resolved = store
            .resolve_confirmation_reference(&reference)
            .unwrap()
            .unwrap();
        assert_eq!(resolved.capsule_id, id);

        let mut tampered = reference.into_bytes();
        let last = tampered.last_mut().unwrap();
        *last = if *last == b'0' { b'1' } else { b'0' };
        let tampered = String::from_utf8(tampered).unwrap();
        assert!(store.resolve_confirmation_reference(&tampered).is_err());
    }
}
