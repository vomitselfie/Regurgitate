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

/// Retrieves the Praxis master key from the platform credential store. On
/// Linux, keyring's v1 provider is the freedesktop Secret Service.
pub struct SecretServiceKeyProvider {
    service: String,
    username: String,
}

impl Default for SecretServiceKeyProvider {
    fn default() -> Self {
        Self {
            service: DEFAULT_SERVICE.to_owned(),
            username: DEFAULT_USERNAME.to_owned(),
        }
    }
}

impl SecretServiceKeyProvider {
    pub fn new(service: impl Into<String>, username: impl Into<String>) -> Result<Self> {
        let service = service.into();
        let username = username.into();
        if service.is_empty() || username.is_empty() {
            bail!("credential store service and username must not be empty");
        }
        Ok(Self { service, username })
    }

    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &self.username)
            .context("Linux Secret Service is unavailable or locked")
    }
}

impl MasterKeyProvider for SecretServiceKeyProvider {
    fn get_or_create(&self) -> Result<MasterKey> {
        let entry = self.entry()?;
        match entry.get_secret() {
            Ok(secret) => {
                let secret = Zeroizing::new(secret);
                MasterKey::from_secret(&secret)
            }
            Err(keyring::Error::NoEntry) => {
                let generated = MasterKey::generate()?;
                entry
                    .set_secret(generated.as_bytes())
                    .context("could not create the Praxis key in Linux Secret Service")?;

                // Re-read the stored value so concurrent first-run writers converge
                // on the credential store's final value.
                let stored = Zeroizing::new(
                    entry
                        .get_secret()
                        .context("could not verify the new Praxis Secret Service key")?,
                );
                MasterKey::from_secret(&stored)
            }
            Err(error) => Err(error).context("Linux Secret Service is unavailable or locked"),
        }
    }
}

impl KeyReadinessProbe for SecretServiceKeyProvider {
    fn key_is_present(&self) -> Result<bool> {
        let entry = self.entry()?;
        match entry.get_secret() {
            Ok(secret) => {
                let secret = Zeroizing::new(secret);
                MasterKey::from_secret(&secret)?;
                Ok(true)
            }
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(error).context("Linux Secret Service is unavailable or locked"),
        }
    }
}
