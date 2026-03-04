use aes_gcm::aead::{Aead, KeyInit, OsRng, rand_core::RngCore};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::digest::KeyInit as HmacKeyInit;
use hmac::Mac;
type HmacSha256 = hmac::Hmac<Sha256>;
use scrypt::Params as ScryptParams;
use sha2::Sha256;
use subtle::ConstantTimeEq;

const SCRYPT_SALT: &[u8] = b"mcp-oauth-proxy";

/// Derive a 32-byte key from a secret using scrypt (N=16384, r=8, p=1).
/// Compatible with Node.js `scryptSync(secret, "mcp-oauth-proxy", 32)`.
fn derive_key(secret: &str) -> [u8; 32] {
    let params = ScryptParams::new(14, 8, 1, 32).expect("valid scrypt params");
    let mut key = [0u8; 32];
    scrypt::scrypt(secret.as_bytes(), SCRYPT_SALT, &params, &mut key).expect("scrypt derivation");
    key
}

/// Encrypt plaintext with AES-256-GCM.
/// Output format: `base64url(iv(12) || tag(16) || ciphertext)`.
/// Compatible with the TypeScript implementation.
pub fn encrypt(plaintext: &str, secret: &str) -> String {
    let key = derive_key(secret);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("valid key length");

    let mut iv_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut iv_bytes);
    let nonce = Nonce::from_slice(&iv_bytes);

    // aes-gcm appends tag after ciphertext: ciphertext || tag(16)
    let ciphertext_with_tag = cipher.encrypt(nonce, plaintext.as_bytes()).expect("encryption");

    let ct_len = ciphertext_with_tag.len() - 16;
    let ciphertext = &ciphertext_with_tag[..ct_len];
    let tag = &ciphertext_with_tag[ct_len..];

    // Reorder to match TypeScript format: iv || tag || ciphertext
    let mut output = Vec::with_capacity(12 + 16 + ct_len);
    output.extend_from_slice(&iv_bytes);
    output.extend_from_slice(tag);
    output.extend_from_slice(ciphertext);

    URL_SAFE_NO_PAD.encode(&output)
}

/// Decrypt a base64url-encoded ciphertext (iv(12) || tag(16) || ciphertext).
/// Compatible with the TypeScript implementation.
pub fn decrypt(ciphertext_b64: &str, secret: &str) -> Result<String, DecryptError> {
    let key = derive_key(secret);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("valid key length");

    let buf = URL_SAFE_NO_PAD
        .decode(ciphertext_b64)
        .map_err(|_| DecryptError)?;

    if buf.len() < 28 {
        return Err(DecryptError);
    }

    let iv = &buf[..12];
    let tag = &buf[12..28];
    let ciphertext = &buf[28..];
    let nonce = Nonce::from_slice(iv);

    // Reconstruct aes-gcm format: ciphertext || tag
    let mut ct_with_tag = Vec::with_capacity(ciphertext.len() + 16);
    ct_with_tag.extend_from_slice(ciphertext);
    ct_with_tag.extend_from_slice(tag);

    let plaintext = cipher
        .decrypt(nonce, ct_with_tag.as_slice())
        .map_err(|_| DecryptError)?;

    String::from_utf8(plaintext).map_err(|_| DecryptError)
}

/// HMAC-SHA256 sign, returning base64url.
pub fn hmac_sign(data: &str, secret: &str) -> String {
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(data.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// Timing-safe HMAC verification.
pub fn hmac_verify(data: &str, signature: &str, secret: &str) -> bool {
    let expected = hmac_sign(data, secret);
    let expected_bytes = expected.as_bytes();
    let actual_bytes = signature.as_bytes();
    expected_bytes.len() == actual_bytes.len()
        && expected_bytes.ct_eq(actual_bytes).into()
}

#[derive(Debug)]
pub struct DecryptError;

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "decryption failed")
    }
}

impl std::error::Error for DecryptError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret = "test-secret-key-for-unit-tests";
        let plaintext = r#"{"client_name":"Claude Code","redirect_uris":["http://127.0.0.1/callback"]}"#;

        let encrypted = encrypt(plaintext, secret);
        let decrypted = decrypt(&encrypted, secret).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_invalid_data() {
        assert!(decrypt("invalid-base64", "secret").is_err());
        assert!(decrypt(&URL_SAFE_NO_PAD.encode(b"short"), "secret").is_err());
    }

    #[test]
    fn test_hmac_sign_verify() {
        let secret = "test-secret";
        let data = "some-client-id";
        let sig = hmac_sign(data, secret);
        assert!(hmac_verify(data, &sig, secret));
        assert!(!hmac_verify(data, "wrong-signature", secret));
    }

    #[test]
    fn test_hmac_verify_timing_safe() {
        let secret = "test-secret";
        let data = "data";
        let sig = hmac_sign(data, secret);
        // Different length should fail
        assert!(!hmac_verify(data, &sig[..sig.len() - 1], secret));
    }
}
