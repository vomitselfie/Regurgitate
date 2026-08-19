use anyhow::{Context, Result, bail};
use zeroize::Zeroizing;

use crate::application::KeyReadinessProbe;

const MASTER_KEY_BYTES: usize = 32;
const DEFAULT_SERVICE: &str = "dev.praxis.history";
const DEFAULT_USERNAME: &str = "master-key-v1";

/// A master key held in zeroizing memory. Debug and clone are intentionally not
/// implemented so callers cannot accidentally log or multiply key material.
pub struct MasterKey(Zeroizing<[u8; MASTER_KEY_BYTES]>);

impl MasterKey {
    pub fn from_bytes(bytes: [u8; MASTER_KEY_BYTES]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; MASTER_KEY_BYTES] {
        &self.0
    }

    fn generate() -> Result<Self> {
        let mut bytes = Zeroizing::new([0_u8; MASTER_KEY_BYTES]);
        getrandom::fill(bytes.as_mut())
            .map_err(|error| anyhow::anyhow!("could not generate Praxis master key: {error}"))?;
        Ok(Self(bytes))
    }

    fn from_secret(secret: &[u8]) -> Result<Self> {
        if secret.len() != MASTER_KEY_BYTES {
            bail!("credential store returned an invalid Praxis master key length");
        }
        let mut bytes = Zeroizing::new([0_u8; MASTER_KEY_BYTES]);
        bytes.copy_from_slice(secret);
        Ok(Self(bytes))
    }
}

pub trait MasterKeyProvider {
    fn get_or_create(&self) -> Result<MasterKey>;
}

pub trait ExistingMasterKeyProvider {
    /// Loads only an existing key. This must never create or replace one.
    fn get_existing(&self) -> Result<Option<MasterKey>>;
}

/// Retrieves the Praxis master key from the operating system's credential
/// store. Keyring's v1 provider uses Secret Service on Linux and Keychain on
/// macOS.
pub struct SystemKeyProvider {
    service: String,
    username: String,
}

impl Default for SystemKeyProvider {
    fn default() -> Self {
        Self {
            service: DEFAULT_SERVICE.to_owned(),
            username: DEFAULT_USERNAME.to_owned(),
        }
    }
}

impl SystemKeyProvider {
    pub fn new(service: impl Into<String>, username: impl Into<String>) -> Result<Self> {
        let service = service.into();
        let username = username.into();
        if service.is_empty() || username.is_empty() {
            bail!("credential store service and username must not be empty");
        }
        Ok(Self { service, username })
    }

    fn entry(&self) -> Result<keyring::Entry> {
        if let Err(error) = keyring::Entry::store_status() {
            bail!("operating system credential store could not initialize: {error}");
        }
        keyring::Entry::new(&self.service, &self.username)
            .context("operating system credential store is unavailable or locked")
    }
}

impl MasterKeyProvider for SystemKeyProvider {
    fn get_or_create(&self) -> Result<MasterKey> {
        if let Some(existing) = self.get_existing()? {
            return Ok(existing);
        }
        let entry = self.entry()?;
        let generated = MasterKey::generate()?;
        entry
            .set_secret(generated.as_bytes())
            .context("could not create the Praxis key in the operating system credential store")?;

        // Re-read the stored value so concurrent first-run writers converge
        // on the credential store's final value.
        let stored = Zeroizing::new(
            entry
                .get_secret()
                .context("could not verify the new Praxis credential-store key")?,
        );
        MasterKey::from_secret(&stored)
    }
}

impl ExistingMasterKeyProvider for SystemKeyProvider {
    fn get_existing(&self) -> Result<Option<MasterKey>> {
        let entry = self.entry()?;
        match entry.get_secret() {
            Ok(secret) => {
                let secret = Zeroizing::new(secret);
                MasterKey::from_secret(&secret).map(Some)
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => {
                Err(error).context("operating system credential store is unavailable or locked")
            }
        }
    }
}

impl KeyReadinessProbe for SystemKeyProvider {
    fn key_is_present(&self) -> Result<bool> {
        Ok(self.get_existing()?.is_some())
    }
}
