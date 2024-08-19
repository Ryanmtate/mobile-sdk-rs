use super::storage_manager::*;
use oid4vp::presentation_exchange::ClaimFormatDesignation;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, info_span};
use uuid::Uuid;

/// Internal prefix for credential keys.
const KEY_PREFIX: &str = "Credential.";

// NOTE: Replacing CredentialType with ClaimFormatDesignation from oid4vp-rs crate.
// Does this make sense or are ClaimFormatDesignation and CredentialType different things?
//
// /// Supported credential formats.
// #[derive(PartialEq, Debug, Serialize, Deserialize)]
// pub enum CredentialFormat {
//     MsoMdoc,
//     JwtVcJson,
//     LdpVc,
//     Other(String), // For ease of expansion.
// }

/// Supported credential types.
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum CredentialType {
    Iso18013_5_1mDl,
    VehicleTitle,
    Other(String), // For ease of expansion.
}

/// An individual credential.
#[derive(Debug, Serialize, Deserialize)]
pub struct Credential {
    id: Uuid,
    format: ClaimFormatDesignation,
    ctype: CredentialType,
    payload: Vec<u8>, // The actual credential.
}

impl Credential {
    /// Create a new credential.
    pub fn new(
        id: Uuid,
        format: ClaimFormatDesignation,
        ctype: CredentialType,
        payload: Vec<u8>,
    ) -> Credential {
        Credential {
            id,
            format,
            ctype,
            payload,
        }
    }
}

/// Verifiable Digital Credential Collection
///
/// This is the main interface to credentials.
pub struct VdcCollection {
    storage: Box<dyn StorageManagerInterface>,
}

#[derive(Error, Debug, uniffi::Error)]
pub enum VdcCollectionError {
    /// Attempt to convert the credential to a serialized form suitable for writing to storage failed.
    #[error("Failed to Serialize Value")]
    SerializeFailed,

    /// Attempting to convert the credential to a deserialized form suitable for runtime use failed.
    #[error("Failed to Deserialize Value")]
    DeserializeFailed,

    /// Attempting to write the credential to storage failed.
    #[error("Failed to Write to Storage")]
    StoreFailed(StorageManagerError),

    /// Attempting to read the credential from storage failed.
    #[error("Failed to Read from Storage")]
    LoadFailed(StorageManagerError),

    /// Attempting to delete a credential from storage failed.
    #[error("Failed to Delete from Storage")]
    DeleteFailed(StorageManagerError),
}

impl VdcCollection {
    /// Create a new credential set.
    pub fn new(engine: Box<dyn StorageManagerInterface>) -> VdcCollection {
        VdcCollection { storage: engine }
    }

    /// Add a credential to the set.
    pub fn add(&self, credential: Credential) -> Result<(), VdcCollectionError> {
        let val = match serde_cbor::to_vec(&credential) {
            Ok(x) => x,
            Err(_) => return Err(VdcCollectionError::SerializeFailed),
        };

        match self.storage.add(self.id_to_key(credential.id), Value(val)) {
            Ok(()) => Ok(()),
            Err(e) => Err(VdcCollectionError::StoreFailed(e)),
        }
    }

    /// Get a credential from the store.
    pub fn get(&self, id: &str) -> Result<Option<Credential>, VdcCollectionError> {
        let raw = match self.storage.get(self.str_to_key(id)) {
            Ok(Some(x)) => x,
            Ok(None) => return Ok(None),
            Err(e) => return Err(VdcCollectionError::LoadFailed(e)),
        };

        match serde_cbor::de::from_slice(&raw.0) {
            Ok(x) => Ok(x),
            Err(_) => Err(VdcCollectionError::DeserializeFailed),
        }
    }

    /// Remove a credential from the store.
    pub fn delete(&self, id: &str) -> Result<(), VdcCollectionError> {
        match self.storage.remove(self.str_to_key(id)) {
            Ok(_) => Ok(()),
            Err(e) => Err(VdcCollectionError::DeleteFailed(e)),
        }
    }

    /// Get a list of all the credentials.
    pub fn all_entries(&self) -> Result<Vec<String>, VdcCollectionError> {
        let mut r = Vec::new();

        match self.storage.list() {
            Ok(list) => {
                for key in list {
                    let name = key.0;

                    if name.starts_with(KEY_PREFIX) {
                        if let Some(id) = name.strip_prefix(KEY_PREFIX) {
                            r.push(id.to_string());
                        }
                    }
                }
            }
            Err(e) => return Err(VdcCollectionError::LoadFailed(e)),
        }

        Ok(r)
    }

    /// Get a list of all the credentials that match a specified type.
    pub fn all_entries_by_type(
        &self,
        ctype: CredentialType,
    ) -> Result<Vec<String>, VdcCollectionError> {
        let mut r = Vec::new();

        match self.all_entries() {
            Ok(list) => {
                for key in list {
                    let cred = self.get(&key);

                    if let Ok(Some(x)) = cred {
                        if x.ctype == ctype {
                            r.push(key);
                        }
                    }
                }
            }
            Err(e) => return Err(e),
        }

        Ok(r)
    }

    /// Convert a UUID to a storage key.
    fn id_to_key(&self, id: Uuid) -> Key {
        self.str_to_key(&id.to_string())
    }

    /// Convert a string ref to a storage key.
    fn str_to_key(&self, id: &str) -> Key {
        Key(format!("{}{}", KEY_PREFIX, id))
    }

    /// Dump the contents of the credential set to the logger.
    pub fn dump(&self) {
        let span = info_span!("All Credentials");
        span.in_scope(|| match self.all_entries() {
            Ok(list) => {
                for key in list {
                    if let Ok(x) = self.get(&key) {
                        info!("{:?}", x);
                    }
                }
            }
            Err(e) => info!("Unable to get list: {:?}", e),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_store::*;
    use uuid::uuid;

    #[test]
    fn test_vdc() {
        let smi = LocalStore;
        let vdc = VdcCollection::new(Box::new(smi));
        let payload_1: Vec<u8> = "Some random collection of bytes. ⚛".into();
        let payload_2: Vec<u8> = "Some other random collection of bytes. 📯".into();
        let payload_3: Vec<u8> = "Some third random collection of bytes. λ".into();

        vdc.add(Credential::new(
            uuid!("00000000-0000-0000-0000-000000000001"),
            ClaimFormatDesignation::MsoMDoc,
            CredentialType::Iso18013_5_1mDl,
            payload_1.clone(),
        ))
        .expect("Unable to add the first value.");

        vdc.add(Credential::new(
            uuid!("00000000-0000-0000-0000-000000000002"),
            ClaimFormatDesignation::MsoMDoc,
            CredentialType::Iso18013_5_1mDl,
            payload_2.clone(),
        ))
        .expect("Unable to add the second value.");

        vdc.add(Credential::new(
            uuid!("00000000-0000-0000-0000-000000000003"),
            ClaimFormatDesignation::MsoMDoc,
            CredentialType::Iso18013_5_1mDl,
            payload_3.clone(),
        ))
        .expect("Unable to add the third value.");

        vdc.get("00000000-0000-0000-0000-000000000002")
            .expect("Failed to get the second value");
        vdc.get("00000000-0000-0000-0000-000000000001")
            .expect("Failed to get the first value");
        vdc.get("00000000-0000-0000-0000-000000000003")
            .expect("Failed to get the third value");

        assert!(vdc.all_entries().unwrap().len() == 3);

        vdc.delete("00000000-0000-0000-0000-000000000002")
            .expect("Failed to delete the second value.");

        assert!(vdc.all_entries().unwrap().len() == 2);

        vdc.delete("00000000-0000-0000-0000-000000000001")
            .expect("Failed to delete the first value.");
        vdc.delete("00000000-0000-0000-0000-000000000003")
            .expect("Failed to delete the third value.");

        assert!(vdc.all_entries().unwrap().len() == 0);
    }
}
