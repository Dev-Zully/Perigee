#![allow(dead_code)]

use sha2::{Digest, Sha256};

pub struct SecureIdGenerator {
    salt: String,
}

impl SecureIdGenerator {
    pub fn new(salt: &str) -> Self {
        Self {
            salt: salt.to_string(),
        }
    }

    pub fn generate_vault_id(&self, manager_id: &str, name: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.salt.as_bytes());
        hasher.update(manager_id.as_bytes());
        hasher.update(name.as_bytes());
        let result = hasher.finalize();
        format!("vault_{}", hex::encode(&result[..16]))
    }

    pub fn generate_secret_ref(&self, vault_id: &str, purpose: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.salt.as_bytes());
        hasher.update(vault_id.as_bytes());
        hasher.update(purpose.as_bytes());
        let result = hasher.finalize();
        format!("secret_{}", hex::encode(&result[..16]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_vault_id_deterministic() {
        let gen = SecureIdGenerator::new("test_salt");
        let id1 = gen.generate_vault_id("mgr1", "My Vault");
        let id2 = gen.generate_vault_id("mgr1", "My Vault");
        assert_eq!(id1, id2);
        assert!(id1.starts_with("vault_"));
    }

    #[test]
    fn test_different_inputs_different_ids() {
        let gen = SecureIdGenerator::new("test_salt");
        let id1 = gen.generate_vault_id("mgr1", "Vault A");
        let id2 = gen.generate_vault_id("mgr1", "Vault B");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_secret_ref() {
        let gen = SecureIdGenerator::new("test_salt");
        let secret = gen.generate_secret_ref("vault_abc", "signing");
        assert!(secret.starts_with("secret_"));
    }

    #[test]
    fn test_different_salt_different_ids() {
        let gen1 = SecureIdGenerator::new("salt1");
        let gen2 = SecureIdGenerator::new("salt2");
        let id1 = gen1.generate_vault_id("mgr1", "Vault");
        let id2 = gen2.generate_vault_id("mgr1", "Vault");
        assert_ne!(id1, id2);
    }
}
