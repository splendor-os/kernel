//! # Content Hashing
//!
//! Splendor uses content hashes to identify state nodes and snapshots in a
//! stable, replay-friendly way. These hashes are serialized alongside state and
//! trace data to ensure reproducible identifiers across process restarts.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_types::{ContentHash, HashAlgorithm};
//!
//! let hash = ContentHash::blake3(b"state bytes");
//! assert_eq!(hash.algorithm, HashAlgorithm::Blake3);
//! assert!(hash.to_string().starts_with("blake3:"));
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Hash algorithms supported by the Splendor kernel.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// BLAKE3 hashing for content-addressed identifiers.
    Blake3,
    /// SHA-256 hashing for SDK-local state references that need portable identity strings.
    Sha256,
}

/// Deterministic content hash used for state and snapshot IDs.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ContentHash {
    /// Algorithm used to produce the hash.
    pub algorithm: HashAlgorithm,
    /// Hex-encoded hash output.
    pub value: String,
}

impl ContentHash {
    /// Builds a new content hash from explicit parts.
    pub fn new(algorithm: HashAlgorithm, value: impl Into<String>) -> Self {
        Self {
            algorithm,
            value: value.into(),
        }
    }

    /// Hashes bytes with BLAKE3 for deterministic identifiers.
    pub fn blake3(bytes: impl AsRef<[u8]>) -> Self {
        let hash = blake3::hash(bytes.as_ref());
        Self::new(HashAlgorithm::Blake3, hash.to_hex().to_string())
    }

    /// Parses the canonical `algorithm:value` string representation.
    pub fn parse(value: &str) -> Option<Self> {
        let (algorithm, digest) = value.split_once(':')?;
        if digest.trim().is_empty() {
            return None;
        }
        Some(Self::new(HashAlgorithm::parse(algorithm)?, digest))
    }
}

impl HashAlgorithm {
    /// Returns the canonical string label for the algorithm.
    pub fn as_str(&self) -> &'static str {
        match self {
            HashAlgorithm::Blake3 => "blake3",
            HashAlgorithm::Sha256 => "sha256",
        }
    }

    /// Parses an algorithm label into the enum.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "blake3" => Some(HashAlgorithm::Blake3),
            "sha256" => Some(HashAlgorithm::Sha256),
            _ => None,
        }
    }
}

impl fmt::Display for ContentHash {
    /// Formats the hash as `algorithm:value`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.algorithm, self.value)
    }
}

impl fmt::Display for HashAlgorithm {
    /// Formats the algorithm using its canonical string label.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "../tests/unit/hash_tests.rs"]
mod tests;
