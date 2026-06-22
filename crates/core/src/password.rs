//! Argon2 password hashing for user credentials.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use crate::error::{AppError, Result};

/// Hash a plaintext password with Argon2id (PHC string format).
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::internal(format!("password hash failed: {err}")))
}

/// Verify a plaintext password against a stored Argon2 PHC hash.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|err| AppError::internal(format!("invalid password hash: {err}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
