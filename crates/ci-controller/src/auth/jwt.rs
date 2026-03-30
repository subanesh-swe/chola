use anyhow::Result;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ci_core::models::user::User;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id (UUID string)
    pub username: String,
    pub role: String,
    pub jti: String, // unique token ID for revocation
    pub exp: usize,  // expiry (Unix timestamp)
    pub iat: usize,  // issued at
}

/// Encode a JWT for the given user. Returns (token_string, jti).
pub fn encode_token(secret: &str, user: &User, expiry_secs: u64) -> Result<(String, String)> {
    let now = Utc::now().timestamp() as usize;
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        sub: user.id.to_string(),
        username: user.username.clone(),
        role: user.role.to_string(),
        jti: jti.clone(),
        exp: now + expiry_secs as usize,
        iat: now,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok((token, jti))
}

/// Decode and validate a JWT token. Returns the claims.
pub fn decode_token(secret: &str, token: &str) -> Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
