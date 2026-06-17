use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Hash a password with argon2id (default params). Returns the PHC string to store.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

/// Verify a password against a stored PHC hash. A malformed stored hash returns false (never panics).
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// A PHC hash of a fixed dummy password, used to flatten timing on unknown-user login (no enumeration).
pub fn dummy_verify(password: &str) {
    // Verify against a constant invalid-credential hash so unknown-user login spends ~the same time.
    let _ = verify_password(password, DUMMY_HASH);
}
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$b3Jqb3Jqb3Jqb3Jqb3Jqb3Jqb3Jqb3Jqb3Jqb3Jqb3I";

/// A random URL-safe 256-bit secret (for session cookie values, API tokens, and record ids).
pub fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 hex of an input (for storing session/token secrets at rest; lookups compare hex equality).
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_password() {
        let h = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn verify_against_garbage_hash_is_false_not_panic() {
        assert!(!verify_password("x", "not-a-hash"));
    }

    #[test]
    fn dummy_verify_does_not_panic() {
        // exercises the argon2 verify path on the unknown-user branch (constant PHC must be valid)
        dummy_verify("anything");
    }

    #[test]
    fn secret_is_url_safe_and_hash_is_stable_hex() {
        let s = random_secret();
        assert!(
            s.len() >= 40
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(sha256_hex("abc"), sha256_hex("abc"));
        assert_ne!(sha256_hex("abc"), sha256_hex("abd"));
        assert_eq!(sha256_hex("abc").len(), 64);
    }
}
