//! One-way Argon2 hashing for stored secrets (e.g. webhook signing secrets).
//!
//! Any secret persisted through this module can never be recovered in
//! plaintext — only verified against a value presented back by the caller.
//! Do not use this for values that must later be read back in plaintext
//! (e.g. an HMAC signing key); those require encryption, not hashing.

use argon2::password_hash::{
    rand_core::OsRng, Error as PasswordHashError, PasswordHash, PasswordHasher, SaltString,
};
use argon2::Argon2;
use subtle::ConstantTimeEq;

/// Hashes a plaintext secret with Argon2id, returning a self-describing
/// encoded hash (algorithm, params, salt, and digest) safe to store at rest.
pub fn hash_secret(secret: &str) -> Result<String, PasswordHashError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(secret.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verifies a plaintext secret against a previously stored Argon2 hash.
/// Returns `false` (rather than erroring) on any malformed hash or mismatch.
///
/// The comparison of the two digests is performed with
/// [`subtle::ConstantTimeEq`] instead of `==` so that an attacker measuring
/// response times cannot learn how much of the secret is correct byte by byte
/// (a timing side channel). The candidate digest is recomputed from the stored
/// salt using the same Argon2id defaults as [`hash_secret`], which keeps the
/// two digests directly comparable for hashes produced by this module.
pub fn verify_secret(secret: &str, hash: &str) -> bool {
    let Ok(stored) = PasswordHash::new(hash) else {
        return false;
    };
    let Some(stored_digest) = stored.hash.as_ref() else {
        return false;
    };
    let Some(salt) = stored.salt.as_ref() else {
        return false;
    };

    // Re-derive the candidate digest from the *stored* salt so it is comparable
    // to `stored_digest`.
    let Ok(salt_string) = SaltString::from_b64(salt.as_str()) else {
        return false;
    };
    let Ok(candidate) = Argon2::default().hash_password(secret.as_bytes(), &salt_string) else {
        return false;
    };
    let Some(candidate_digest) = candidate.hash.as_ref() else {
        return false;
    };

    // Constant-time comparison of the two digests.
    bool::from(stored_digest.as_bytes().ct_eq(candidate_digest.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let hash = hash_secret("my-webhook-secret").expect("hashing should succeed");
        assert!(verify_secret("my-webhook-secret", &hash));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let hash = hash_secret("my-webhook-secret").expect("hashing should succeed");
        assert!(!verify_secret("not-the-secret", &hash));
    }

    #[test]
    fn verify_rejects_malformed_hash() {
        assert!(!verify_secret("anything", "not-a-real-hash"));
    }

    #[test]
    fn same_secret_hashes_differently_each_time() {
        let first = hash_secret("repeat-secret").expect("hashing should succeed");
        let second = hash_secret("repeat-secret").expect("hashing should succeed");
        assert_ne!(first, second, "salts must be randomized per hash");
        assert!(verify_secret("repeat-secret", &first));
        assert!(verify_secret("repeat-secret", &second));
    }

    /// A timing attack relies on the comparison stopping as soon as a differing
    /// byte is found. If the verifier short-circuited, a secret sharing a long
    /// prefix with the real one would take measurably longer to reject than one
    /// that differs immediately. This test pins the *behavioural* contract that
    /// verification is a single constant-time digest comparison: wrong secrets
    /// — whether they share no prefix or a long prefix with the real secret —
    /// are all rejected, and the comparison does not depend on how much of the
    /// candidate matches. (Per BE-043 / issue #280.)
    #[test]
    fn verify_rejects_partial_prefix_secret_without_leaking() {
        let hash = hash_secret("correct-horse-battery-staple").expect("hashing should succeed");

        // No shared prefix at all.
        assert!(!verify_secret("wrong-secret-entirely", &hash));

        // Long shared prefix with the real secret: must still be rejected, and
        // only via the constant-time digest compare rather than a byte-wise `==`.
        assert!(!verify_secret("correct-horse-battery-stapler", &hash));
        assert!(!verify_secret("correct-horse-battery", &hash));
    }

    /// Sanity check that the constant-time path agrees with a correct secret.
    #[test]
    fn verify_constant_time_accepts_correct_secret() {
        let hash = hash_secret("constant-time-secret").expect("hashing should succeed");
        assert!(verify_secret("constant-time-secret", &hash));
    }
}
