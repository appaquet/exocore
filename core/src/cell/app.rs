use super::Error;
use crate::crypto::keys::PublicKey;
use crate::protos::generated::exocore_apps::Manifest;
use std::sync::Arc;

#[derive(Clone)]
pub struct Application {
    identity: Arc<Identity>,
}

struct Identity {
    public_key: PublicKey,
    id: ApplicationId,
    name: String,
}

impl Application {
    pub fn new_from_manifest(manifest: &Manifest) -> Result<Application, Error> {
        let public_key = PublicKey::decode_base58_string(&manifest.public_key).map_err(|err| {
            Error::Config(format!("Error parsing application public_key: {}", err))
        })?;

        Ok(Self::build(public_key, manifest.name.clone()))
    }

    fn build(public_key: PublicKey, name: String) -> Application {
        let id = ApplicationId::from_public_key(&public_key);

        Application {
            identity: Arc::new(Identity {
                public_key,
                id,
                name,
            }),
        }
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.identity.public_key
    }

    pub fn id(&self) -> &ApplicationId {
        &self.identity.id
    }

    pub fn name(&self) -> &str {
        &self.identity.name
    }
}

/// Unique identifier of an application, which is built by hashing the public key
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct ApplicationId(String);

impl ApplicationId {
    pub fn from_public_key(public_key: &PublicKey) -> ApplicationId {
        let id = public_key.encode_base58_string();
        ApplicationId(id)
    }

    pub fn from_string(id: String) -> ApplicationId {
        ApplicationId(id)
    }

    pub fn from_bytes(id: &[u8]) -> ApplicationId {
        ApplicationId(String::from_utf8_lossy(id).to_string())
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for ApplicationId {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::str::FromStr for ApplicationId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ApplicationId(s.to_string()))
    }
}
