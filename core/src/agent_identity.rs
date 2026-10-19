use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const ED25519_PUBKEY_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub public_key: Option<Vec<u8>>,
    pub reputation_score: u32,
    pub is_active: bool,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IdentityError {
    InvalidPublicKey(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::InvalidPublicKey(msg) => write!(f, "Invalid public key: {}", msg),
        }
    }
}

impl AgentIdentity {
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            public_key: None,
            reputation_score: 0,
            is_active: true,
            revoked_at: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_with_public_key(
        agent_id: String,
        public_key: Vec<u8>,
    ) -> Result<Self, IdentityError> {
        Self::validate_public_key(&public_key)?;
        Ok(Self {
            agent_id,
            public_key: Some(public_key),
            reputation_score: 0,
            is_active: true,
            revoked_at: None,
            created_at: Utc::now(),
        })
    }

    pub fn set_public_key(&mut self, key: Vec<u8>) -> Result<(), IdentityError> {
        Self::validate_public_key(&key)?;
        self.public_key = Some(key);
        Ok(())
    }

    pub fn validate_public_key(key: &[u8]) -> Result<(), IdentityError> {
        if key.len() != ED25519_PUBKEY_LEN {
            return Err(IdentityError::InvalidPublicKey(format!(
                "expected {} bytes, got {}",
                ED25519_PUBKEY_LEN,
                key.len()
            )));
        }
        Ok(())
    }

    pub fn revoke(&mut self) {
        self.is_active = false;
        self.revoked_at = Some(Utc::now());
        crate::audit_log::log_security_event(
            crate::audit_log::SecurityEventType::AgentDisabled,
            Some(&self.agent_id),
            None,
            None,
            Some("Agent identity revoked and disabled"),
        );
    }

    pub fn is_revoked(&self) -> bool {
        !self.is_active && self.revoked_at.is_some()
    }

    pub fn was_revoked_before(&self, dt: DateTime<Utc>) -> bool {
        match self.revoked_at {
            Some(revoked) => revoked < dt,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_identity() {
        let id = AgentIdentity::new("agent-1".to_string());
        assert_eq!(id.agent_id, "agent-1");
        assert!(id.is_active);
        assert!(id.revoked_at.is_none());
        assert_eq!(id.reputation_score, 0);
    }

    #[test]
    fn test_revoke() {
        let mut id = AgentIdentity::new("agent-1".to_string());
        assert!(!id.is_revoked());
        id.revoke();
        assert!(id.is_revoked());
        assert!(!id.is_active);
        assert!(id.revoked_at.is_some());
    }

    #[test]
    fn test_was_revoked_before() {
        let mut id = AgentIdentity::new("agent-1".to_string());
        let now = Utc::now();
        assert!(!id.was_revoked_before(now));
        id.revoke();
        let future = Utc::now() + chrono::Duration::hours(1);
        assert!(id.was_revoked_before(future));
        let past = Utc::now() - chrono::Duration::hours(1);
        assert!(!id.was_revoked_before(past));
    }

    #[test]
    fn test_valid_public_key() {
        let key = vec![0u8; 32];
        let id = AgentIdentity::new_with_public_key("agent-1".to_string(), key);
        assert!(id.is_ok());
    }

    #[test]
    fn test_invalid_public_key_too_short() {
        let key = vec![0u8; 16];
        let id = AgentIdentity::new_with_public_key("agent-1".to_string(), key);
        assert!(id.is_err());
    }

    #[test]
    fn test_invalid_public_key_too_long() {
        let key = vec![0u8; 64];
        let id = AgentIdentity::new_with_public_key("agent-1".to_string(), key);
        assert!(id.is_err());
    }

    #[test]
    fn test_set_public_key_validates() {
        let mut id = AgentIdentity::new("agent-1".to_string());
        assert!(id.set_public_key(vec![0u8; 32]).is_ok());
        assert!(id.set_public_key(vec![0u8; 16]).is_err());
    }
}
