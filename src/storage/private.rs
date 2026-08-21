use anyhow::{Result, anyhow, bail};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, Generate, KeyInit as AeadKeyInit, Payload},
};
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use zeroize::Zeroizing;

use super::MasterKey;

pub(super) const PRIVATE_ENVELOPE_VERSION: u32 = 1;
const PRIVATE_KEY_INFO: &[u8] = b"regurgitate:private-metadata-encryption:v1";
const LOOKUP_KEY_INFO: &[u8] = b"regurgitate:private-metadata-lookup:v1";
const AAD_DOMAIN: &[u8] = b"regurgitate:private-metadata-envelope";
const NONCE_BYTES: usize = 24;

#[derive(Clone, Copy)]
pub(super) enum PrivateRecordKind {
    Project = 1,
    Cursor = 2,
    EventProject = 3,
    ExperienceScope = 4,
}

pub(super) struct SealedPrivateRecord {
    pub nonce: [u8; NONCE_BYTES],
    pub ciphertext: Vec<u8>,
}

pub(super) struct PrivateMetadataCipher {
    cipher: XChaCha20Poly1305,
    lookup_key: Zeroizing<[u8; 32]>,
}

impl PrivateMetadataCipher {
    pub fn new(master_key: &MasterKey) -> Result<Self> {
        let hkdf = Hkdf::<Sha256>::new(None, master_key.as_bytes());
        let mut encryption_key = Zeroizing::new([0_u8; 32]);
        hkdf.expand(PRIVATE_KEY_INFO, encryption_key.as_mut())
            .map_err(|_| anyhow!("could not derive the Regurgitate private metadata key"))?;
        let cipher = XChaCha20Poly1305::new_from_slice(encryption_key.as_ref()).map_err(|_| {
            anyhow!("derived Regurgitate private metadata key has an invalid length")
        })?;

        let mut lookup_key = Zeroizing::new([0_u8; 32]);
        hkdf.expand(LOOKUP_KEY_INFO, lookup_key.as_mut())
            .map_err(|_| anyhow!("could not derive the Regurgitate metadata lookup key"))?;
        Ok(Self { cipher, lookup_key })
    }

    pub fn lookup_token(&self, kind: PrivateRecordKind, identity: &[u8]) -> Result<[u8; 32]> {
        let mut mac = <Hmac<Sha256> as HmacKeyInit>::new_from_slice(self.lookup_key.as_ref())
            .map_err(|_| {
                anyhow!("derived Regurgitate metadata lookup key has an invalid length")
            })?;
        mac.update(AAD_DOMAIN);
        mac.update(&[kind as u8]);
        mac.update(&(identity.len() as u64).to_be_bytes());
        mac.update(identity);
        Ok(mac.finalize().into_bytes().into())
    }

    pub fn seal<T: Serialize>(
        &self,
        kind: PrivateRecordKind,
        lookup_token: &[u8; 32],
        value: &T,
    ) -> Result<SealedPrivateRecord> {
        let mut plaintext = Zeroizing::new(Vec::new());
        ciborium::into_writer(value, &mut *plaintext)
            .map_err(|error| anyhow!("could not serialize private metadata as CBOR: {error}"))?;
        let nonce = XNonce::generate();
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: &associated_data(kind, lookup_token),
                },
            )
            .map_err(|_| anyhow!("could not encrypt Regurgitate private metadata"))?;
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        nonce_bytes.copy_from_slice(&nonce);
        Ok(SealedPrivateRecord {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn open<T: DeserializeOwned>(
        &self,
        kind: PrivateRecordKind,
        lookup_token: &[u8; 32],
        envelope_version: u32,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<T> {
        if envelope_version != PRIVATE_ENVELOPE_VERSION {
            bail!("unsupported Regurgitate private metadata envelope version {envelope_version}");
        }
        let nonce = XNonce::try_from(nonce)
            .map_err(|_| anyhow!("invalid private metadata nonce length"))?;
        let plaintext = self
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: &associated_data(kind, lookup_token),
                },
            )
            .map_err(|_| anyhow!("private metadata authentication failed"))?;
        let plaintext = Zeroizing::new(plaintext);
        ciborium::from_reader(plaintext.as_slice())
            .map_err(|error| anyhow!("could not deserialize private metadata: {error}"))
    }
}

fn associated_data(kind: PrivateRecordKind, lookup_token: &[u8; 32]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(AAD_DOMAIN.len() + 1 + 4 + lookup_token.len());
    aad.extend_from_slice(AAD_DOMAIN);
    aad.push(kind as u8);
    aad.extend_from_slice(&PRIVATE_ENVELOPE_VERSION.to_be_bytes());
    aad.extend_from_slice(lookup_token);
    aad
}
