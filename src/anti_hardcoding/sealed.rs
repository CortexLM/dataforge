//! Sealed parameters for encryption at rest.
//!
//! This module provides functionality to seal (encrypt) benchmark parameters
//! so they cannot be read until verification time. Uses XOR-based encryption
//! with base64 encoding for simplicity (not cryptographically secure, but
//! sufficient for benchmark purposes to prevent casual inspection).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during sealing/unsealing operations.
#[derive(Debug, Error)]
pub enum SealError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("Invalid sealed data format")]
    InvalidFormat,

    #[error("Key must not be empty")]
    EmptyKey,
}

/// Sealed parameters container for encrypted benchmark data.
///
/// Provides methods to seal (encrypt) and unseal (decrypt) parameters
/// using a simple XOR cipher. This is intentionally not cryptographically
/// secure - it's designed to prevent casual inspection of benchmark
/// parameters during task generation, not to protect against determined
/// attackers.
pub struct SealedParameters;

impl SealedParameters {
    /// Seal parameters with a key (base64 encoded result).
    ///
    /// Serializes the parameters to JSON, encrypts using XOR with the key,
    /// and returns the result as a base64-encoded string.
    ///
    /// # Arguments
    /// * `params` - HashMap of parameter names to JSON values
    /// * `key` - Encryption key bytes
    ///
    /// # Returns
    /// Base64-encoded sealed parameters string
    ///
    /// # Errors
    /// Returns `SealError` if serialization fails or key is empty
    pub fn seal(
        params: &HashMap<String, serde_json::Value>,
        key: &[u8],
    ) -> Result<String, SealError> {
        if key.is_empty() {
            return Err(SealError::EmptyKey);
        }

        // Serialize to JSON
        let json_bytes = serde_json::to_vec(params)?;

        // XOR encrypt
        let encrypted = xor_cipher(&json_bytes, key);

        // Base64 encode
        Ok(BASE64.encode(encrypted))
    }

    /// Unseal parameters at verification time.
    ///
    /// Decodes the base64 string, decrypts using XOR with the key,
    /// and deserializes back to a HashMap.
    ///
    /// # Arguments
    /// * `sealed` - Base64-encoded sealed parameters string
    /// * `key` - Decryption key bytes (must match encryption key)
    ///
    /// # Returns
    /// HashMap of parameter names to JSON values
    ///
    /// # Errors
    /// Returns `SealError` if decoding, decryption, or deserialization fails
    pub fn unseal(
        sealed: &str,
        key: &[u8],
    ) -> Result<HashMap<String, serde_json::Value>, SealError> {
        if key.is_empty() {
            return Err(SealError::EmptyKey);
        }

        // Base64 decode
        let encrypted = BASE64.decode(sealed)?;

        // XOR decrypt (same operation as encrypt for XOR)
        let decrypted = xor_cipher(&encrypted, key);

        // Deserialize from JSON
        let params: HashMap<String, serde_json::Value> = serde_json::from_slice(&decrypted)?;

        Ok(params)
    }
}

/// XOR cipher implementation.
///
/// Applies XOR operation between data and key bytes. The key is repeated
/// cyclically if shorter than the data.
fn xor_cipher(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &byte)| byte ^ key[i % key.len()])
        .collect()
}

/// Wrapper struct for serializable sealed data with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedData {
    /// The sealed (encrypted) parameter data as base64 string
    pub sealed: String,
    /// Version of the sealing format
    pub version: u32,
    /// Optional description of what's sealed
    pub description: Option<String>,
}

impl SealedData {
    /// Create new sealed data from parameters.
    ///
    /// # Arguments
    /// * `params` - HashMap of parameter names to JSON values
    /// * `key` - Encryption key bytes
    /// * `description` - Optional description of the sealed data
    ///
    /// # Returns
    /// A `SealedData` struct containing the encrypted parameters
    ///
    /// # Errors
    /// Returns `SealError` if sealing fails
    pub fn new(
        params: &HashMap<String, serde_json::Value>,
        key: &[u8],
        description: Option<String>,
    ) -> Result<Self, SealError> {
        let sealed = SealedParameters::seal(params, key)?;
        Ok(Self {
            sealed,
            version: 1,
            description,
        })
    }

    /// Unseal the data and return the parameters.
    ///
    /// # Arguments
    /// * `key` - Decryption key bytes
    ///
    /// # Returns
    /// HashMap of parameter names to JSON values
    ///
    /// # Errors
    /// Returns `SealError` if unsealing fails
    pub fn unseal(&self, key: &[u8]) -> Result<HashMap<String, serde_json::Value>, SealError> {
        SealedParameters::unseal(&self.sealed, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_seal_and_unseal_roundtrip() {
        let mut params = HashMap::new();
        params.insert("key1".to_string(), json!("value1"));
        params.insert("key2".to_string(), json!(42));
        params.insert("key3".to_string(), json!(true));

        let key = b"test-encryption-key";

        let sealed = SealedParameters::seal(&params, key).expect("sealing should succeed");
        let unsealed = SealedParameters::unseal(&sealed, key).expect("unsealing should succeed");

        assert_eq!(params, unsealed);
    }

    #[test]
    fn test_seal_produces_base64() {
        let mut params = HashMap::new();
        params.insert("test".to_string(), json!("value"));

        let key = b"key";
        let sealed = SealedParameters::seal(&params, key).expect("sealing should succeed");

        // Verify it's valid base64
        assert!(BASE64.decode(&sealed).is_ok());
    }

    #[test]
    fn test_unseal_with_wrong_key_fails() {
        let mut params = HashMap::new();
        params.insert("test".to_string(), json!("value"));

        let seal_key = b"correct-key";
        let wrong_key = b"wrong-key";

        let sealed = SealedParameters::seal(&params, seal_key).expect("sealing should succeed");

        // Unsealing with wrong key should fail (invalid JSON after decryption)
        let result = SealedParameters::unseal(&sealed, wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_key_error() {
        let params = HashMap::new();
        let empty_key: &[u8] = &[];

        let result = SealedParameters::seal(&params, empty_key);
        assert!(matches!(result, Err(SealError::EmptyKey)));
    }

    #[test]
    fn test_sealed_data_with_metadata() {
        let mut params = HashMap::new();
        params.insert("param".to_string(), json!("value"));

        let key = b"test-key";
        let description = Some("Test sealed data".to_string());

        let sealed_data =
            SealedData::new(&params, key, description.clone()).expect("sealing should succeed");

        assert_eq!(sealed_data.version, 1);
        assert_eq!(sealed_data.description, description);

        let unsealed = sealed_data.unseal(key).expect("unsealing should succeed");
        assert_eq!(params, unsealed);
    }

    #[test]
    fn test_complex_nested_params() {
        let mut params = HashMap::new();
        params.insert(
            "nested".to_string(),
            json!({
                "array": [1, 2, 3],
                "object": {"inner": "value"},
                "null": null
            }),
        );

        let key = b"complex-key";

        let sealed = SealedParameters::seal(&params, key).expect("sealing should succeed");
        let unsealed = SealedParameters::unseal(&sealed, key).expect("unsealing should succeed");

        assert_eq!(params, unsealed);
    }

    #[test]
    fn test_xor_cipher_is_reversible() {
        let data = b"Hello, World!";
        let key = b"secret";

        let encrypted = xor_cipher(data, key);
        let decrypted = xor_cipher(&encrypted, key);

        assert_eq!(data.to_vec(), decrypted);
    }
}
