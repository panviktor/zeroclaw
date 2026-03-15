//! Ed25519 agent identity for Phase 3B message-level signing.
//!
//! Each agent generates an Ed25519 keypair locally. The private key never
//! leaves the agent's host. The public key is registered with the broker
//! during pairing or ephemeral provisioning.
//!
//! For ephemeral agents, the broker issues a delegation certificate:
//! `{child_agent_id, child_pubkey, parent_agent_id, expires_at}` signed
//! by the broker's own key.

use ring::signature::{self, Ed25519KeyPair, KeyPair};
use std::path::Path;

/// An agent's Ed25519 identity (keypair + public key bytes).
pub struct AgentIdentity {
    keypair: Ed25519KeyPair,
}

/// A delegation certificate issued by the broker for ephemeral agents.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DelegationCertificate {
    pub child_agent_id: String,
    pub child_pubkey: String, // hex-encoded
    pub parent_agent_id: String,
    pub expires_at: i64,
    pub broker_signature: String, // hex-encoded
}

impl AgentIdentity {
    /// Generate a new Ed25519 keypair using the system CSPRNG.
    pub fn generate() -> Result<Self, String> {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|_| "Failed to generate Ed25519 keypair".to_string())?;
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
            .map_err(|_| "Failed to parse generated keypair".to_string())?;
        Ok(Self { keypair })
    }

    /// Load a keypair from a PKCS#8 DER file, or generate and save a new one.
    pub fn load_or_generate(path: &Path) -> Result<Self, String> {
        if path.exists() {
            let der = std::fs::read(path)
                .map_err(|e| format!("Failed to read key at {}: {e}", path.display()))?;
            // Fix permissions on existing key files that may have been created
            // before the 0o600 hardening was added.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(path) {
                    if meta.permissions().mode() & 0o077 != 0 {
                        tracing::warn!(
                            "Ed25519 key file {} has overly permissive mode {:o}, restricting to 0600",
                            path.display(),
                            meta.permissions().mode() & 0o777
                        );
                        let _ =
                            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
                    }
                }
            }
            let keypair = Ed25519KeyPair::from_pkcs8(&der)
                .map_err(|e| format!("Invalid Ed25519 key at {}: {e}", path.display()))?;
            Ok(Self { keypair })
        } else {
            let rng = ring::rand::SystemRandom::new();
            let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|_| "Failed to generate Ed25519 keypair".to_string())?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create key directory: {e}"))?;
            }
            std::fs::write(path, pkcs8_bytes.as_ref())
                .map_err(|e| format!("Failed to write key to {}: {e}", path.display()))?;
            // Restrict key file permissions to owner-only (0o600).
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
            let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
                .map_err(|_| "Generated key failed to reload".to_string())?;
            Ok(Self { keypair })
        }
    }

    /// Load a keypair from raw PKCS#8 DER bytes (for ephemeral agents).
    pub fn from_pkcs8(der: &[u8]) -> Result<Self, String> {
        let keypair =
            Ed25519KeyPair::from_pkcs8(der).map_err(|e| format!("Invalid PKCS#8 key: {e}"))?;
        Ok(Self { keypair })
    }

    /// Get the public key as hex-encoded bytes.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.keypair.public_key().as_ref())
    }

    /// Get the raw public key bytes.
    pub fn public_key_bytes(&self) -> &[u8] {
        self.keypair.public_key().as_ref()
    }

    /// Sign a message. Returns hex-encoded signature.
    pub fn sign(&self, message: &[u8]) -> String {
        let sig = self.keypair.sign(message);
        hex::encode(sig.as_ref())
    }

    /// Build the canonical signing payload for an IPC message.
    ///
    /// Format: `{from_agent}|{to_agent}|{seq}|{timestamp}|{payload_sha256}`
    ///
    /// This must match the format used in `agents_ipc.rs` (client-side signing)
    /// and `gateway/ipc.rs` (broker-side verification).
    pub fn build_signing_payload(
        from_agent: &str,
        to_agent: &str,
        seq: i64,
        timestamp: i64,
        payload: &str,
    ) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let payload_hash = hex::encode(Sha256::digest(payload.as_bytes()));
        format!("{from_agent}|{to_agent}|{seq}|{timestamp}|{payload_hash}").into_bytes()
    }

    /// Sign an IPC message payload.
    pub fn sign_message(
        &self,
        from_agent: &str,
        to_agent: &str,
        seq: i64,
        timestamp: i64,
        payload: &str,
    ) -> String {
        let data = Self::build_signing_payload(from_agent, to_agent, seq, timestamp, payload);
        self.sign(&data)
    }
}

/// Verify an Ed25519 signature against a hex-encoded public key.
pub fn verify_signature(
    public_key_hex: &str,
    message: &[u8],
    signature_hex: &str,
) -> Result<(), String> {
    let pub_bytes =
        hex::decode(public_key_hex).map_err(|e| format!("Invalid public key hex: {e}"))?;
    let sig_bytes =
        hex::decode(signature_hex).map_err(|e| format!("Invalid signature hex: {e}"))?;

    let public_key = signature::UnparsedPublicKey::new(&signature::ED25519, &pub_bytes);
    public_key
        .verify(message, &sig_bytes)
        .map_err(|_| "Signature verification failed".to_string())
}

/// Verify an IPC message signature.
pub fn verify_message_signature(
    public_key_hex: &str,
    from_agent: &str,
    to_agent: &str,
    seq: i64,
    timestamp: i64,
    payload: &str,
    signature_hex: &str,
) -> Result<(), String> {
    let data = AgentIdentity::build_signing_payload(from_agent, to_agent, seq, timestamp, payload);
    verify_signature(public_key_hex, &data, signature_hex)
}

/// Sign data with a broker's key for delegation certificates.
pub fn sign_delegation_certificate(
    broker_identity: &AgentIdentity,
    child_agent_id: &str,
    child_pubkey_hex: &str,
    parent_agent_id: &str,
    expires_at: i64,
) -> DelegationCertificate {
    let cert_data = format!("{child_agent_id}|{child_pubkey_hex}|{parent_agent_id}|{expires_at}");
    let broker_signature = broker_identity.sign(cert_data.as_bytes());

    DelegationCertificate {
        child_agent_id: child_agent_id.to_string(),
        child_pubkey: child_pubkey_hex.to_string(),
        parent_agent_id: parent_agent_id.to_string(),
        expires_at,
        broker_signature,
    }
}

/// Verify a delegation certificate against the broker's public key.
pub fn verify_delegation_certificate(
    broker_pubkey_hex: &str,
    cert: &DelegationCertificate,
) -> Result<(), String> {
    let cert_data = format!(
        "{}|{}|{}|{}",
        cert.child_agent_id, cert.child_pubkey, cert.parent_agent_id, cert.expires_at
    );
    verify_signature(
        broker_pubkey_hex,
        cert_data.as_bytes(),
        &cert.broker_signature,
    )?;

    // Check expiry
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    if now > cert.expires_at {
        return Err("Delegation certificate has expired".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sign_verify() {
        let identity = AgentIdentity::generate().unwrap();
        let message = b"hello world";
        let sig = identity.sign(message);

        assert!(verify_signature(&identity.public_key_hex(), message, &sig).is_ok());
    }

    #[test]
    fn tampered_message_fails_verification() {
        let identity = AgentIdentity::generate().unwrap();
        let sig = identity.sign(b"original");

        let result = verify_signature(&identity.public_key_hex(), b"tampered", &sig);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_key_fails_verification() {
        let identity1 = AgentIdentity::generate().unwrap();
        let identity2 = AgentIdentity::generate().unwrap();
        let sig = identity1.sign(b"message");

        let result = verify_signature(&identity2.public_key_hex(), b"message", &sig);
        assert!(result.is_err());
    }

    #[test]
    fn ipc_message_sign_verify_roundtrip() {
        let identity = AgentIdentity::generate().unwrap();
        let sig = identity.sign_message("opus", "sentinel", 42, 1_700_000_000, "check status");

        assert!(verify_message_signature(
            &identity.public_key_hex(),
            "opus",
            "sentinel",
            42,
            1_700_000_000,
            "check status",
            &sig
        )
        .is_ok());
    }

    #[test]
    fn ipc_message_wrong_payload_fails() {
        let identity = AgentIdentity::generate().unwrap();
        let sig = identity.sign_message("opus", "sentinel", 42, 1_700_000_000, "check status");

        let result = verify_message_signature(
            &identity.public_key_hex(),
            "opus",
            "sentinel",
            42,
            1_700_000_000,
            "modified payload",
            &sig,
        );
        assert!(result.is_err());
    }

    #[test]
    fn ipc_message_wrong_seq_fails() {
        let identity = AgentIdentity::generate().unwrap();
        let sig = identity.sign_message("opus", "sentinel", 42, 1_700_000_000, "check status");

        let result = verify_message_signature(
            &identity.public_key_hex(),
            "opus",
            "sentinel",
            99, // wrong seq
            1_700_000_000,
            "check status",
            &sig,
        );
        assert!(result.is_err());
    }

    #[test]
    fn delegation_certificate_roundtrip() {
        let broker = AgentIdentity::generate().unwrap();
        let child = AgentIdentity::generate().unwrap();
        let expires = 9_999_999_999i64;

        let cert = sign_delegation_certificate(
            &broker,
            "eph-opus-abc",
            &child.public_key_hex(),
            "opus",
            expires,
        );

        assert!(verify_delegation_certificate(&broker.public_key_hex(), &cert).is_ok());
    }

    #[test]
    fn delegation_certificate_tampered_fails() {
        let broker = AgentIdentity::generate().unwrap();
        let child = AgentIdentity::generate().unwrap();

        let mut cert = sign_delegation_certificate(
            &broker,
            "eph-opus-abc",
            &child.public_key_hex(),
            "opus",
            9_999_999_999,
        );
        cert.child_agent_id = "eph-attacker-xyz".to_string();

        assert!(verify_delegation_certificate(&broker.public_key_hex(), &cert).is_err());
    }

    #[test]
    fn delegation_certificate_expired_fails() {
        let broker = AgentIdentity::generate().unwrap();
        let child = AgentIdentity::generate().unwrap();

        let cert = sign_delegation_certificate(
            &broker,
            "eph-opus-abc",
            &child.public_key_hex(),
            "opus",
            1, // expired
        );

        let result = verify_delegation_certificate(&broker.public_key_hex(), &cert);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expired"));
    }

    #[test]
    fn load_or_generate_creates_and_reloads() {
        let tmp = tempfile::TempDir::new().unwrap();
        let key_path = tmp.path().join("agent.key");

        let id1 = AgentIdentity::load_or_generate(&key_path).unwrap();
        let pubkey1 = id1.public_key_hex();

        let id2 = AgentIdentity::load_or_generate(&key_path).unwrap();
        let pubkey2 = id2.public_key_hex();

        assert_eq!(pubkey1, pubkey2, "Reloaded key must match original");

        // Signature from id1 verifiable with id2's pubkey
        let sig = id1.sign(b"test");
        assert!(verify_signature(&pubkey2, b"test", &sig).is_ok());
    }

    #[test]
    fn public_key_hex_is_valid_hex() {
        let identity = AgentIdentity::generate().unwrap();
        let hex = identity.public_key_hex();
        assert_eq!(hex.len(), 64); // Ed25519 pubkey = 32 bytes = 64 hex chars
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
