//! wallet2's payload encryption (`encrypt_with_view_secret_key`) and Monero's
//! Schnorr-style `crypto::generate_signature` / `check_signature`.
//!
//! key = cn_slow_hash(view_secret_key)        (CryptoNight v0, kdf_rounds = 1)
//! blob = iv[8] || chacha20(key, iv, plaintext) || signature[64]
//! signature = generate_signature(keccak256(iv || ciphertext), view_public, view_secret)

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20Legacy;
use cuprate_cryptonight::cryptonight_hash_v0;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::{Identity, IsIdentity};
use rand_core::{CryptoRng, RngCore};
use sha3::{Digest, Keccak256};
use zeroize::Zeroizing;

use crate::SignerError;

pub const IV_LEN: usize = 8;
pub const SIG_LEN: usize = 64;

/// keccak256, as monero's `cn_fast_hash`.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    Keccak256::digest(data).into()
}

/// keccak256 reduced mod l, as monero's `hash_to_scalar`.
pub fn hash_to_scalar(data: &[u8]) -> Scalar {
    Scalar::from_bytes_mod_order(keccak256(data))
}

/// CryptoNight v0 of the secret key bytes: the chacha key wallet2 derives
/// with `generate_chacha_key(&skey, 32, key, kdf_rounds = 1)`.
pub fn chacha_key_from_secret(secret: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    Zeroizing::new(cryptonight_hash_v0(secret))
}

/// A Monero signature (c, r), 64 bytes on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature {
    pub c: Scalar,
    pub r: Scalar,
}

impl Signature {
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&self.c.to_bytes());
        out[32..].copy_from_slice(&self.r.to_bytes());
        out
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != 64 {
            return None;
        }
        let mut c = [0u8; 32];
        let mut r = [0u8; 32];
        c.copy_from_slice(&b[..32]);
        r.copy_from_slice(&b[32..]);
        Some(Signature {
            c: Option::from(Scalar::from_canonical_bytes(c))?,
            r: Option::from(Scalar::from_canonical_bytes(r))?,
        })
    }
}

/// `crypto::generate_signature(prefix_hash, pub, sec)`.
pub fn generate_signature(
    rng: &mut (impl RngCore + CryptoRng),
    prefix_hash: &[u8; 32],
    public: &EdwardsPoint,
    secret: &Scalar,
) -> Signature {
    let pub_bytes = public.compress().to_bytes();
    loop {
        let k = Zeroizing::new(Scalar::random(rng));
        let comm = (&*k * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        let mut buf = [0u8; 96];
        buf[..32].copy_from_slice(prefix_hash);
        buf[32..64].copy_from_slice(&pub_bytes);
        buf[64..].copy_from_slice(&comm);
        let c = hash_to_scalar(&buf);
        if c == Scalar::ZERO {
            continue;
        }
        let r = &*k - c * secret;
        if r == Scalar::ZERO {
            continue;
        }
        return Signature { c, r };
    }
}

/// `crypto::check_signature(prefix_hash, pub, sig)`.
pub fn check_signature(prefix_hash: &[u8; 32], public: &EdwardsPoint, sig: &Signature) -> bool {
    if sig.c == Scalar::ZERO {
        return false;
    }
    let comm = EdwardsPoint::vartime_double_scalar_mul_basepoint(&sig.c, public, &sig.r);
    if comm.is_identity() {
        return false;
    }
    let mut buf = [0u8; 96];
    buf[..32].copy_from_slice(prefix_hash);
    buf[32..64].copy_from_slice(&public.compress().to_bytes());
    buf[64..].copy_from_slice(&comm.compress().to_bytes());
    hash_to_scalar(&buf) == sig.c
}

/// View-key pair used for the wrapper.
pub struct ViewKey<'a> {
    pub secret: &'a Scalar,
    pub public: EdwardsPoint,
}

impl<'a> ViewKey<'a> {
    pub fn new(secret: &'a Scalar) -> Self {
        ViewKey { secret, public: secret * ED25519_BASEPOINT_TABLE }
    }
}

/// `wallet2::encrypt_with_view_secret_key(plaintext, authenticated = true)`.
pub fn encrypt(rng: &mut (impl RngCore + CryptoRng), view: &ViewKey, plaintext: &[u8]) -> Vec<u8> {
    let key = chacha_key_from_secret(&view.secret.to_bytes());
    let mut iv = [0u8; IV_LEN];
    rng.fill_bytes(&mut iv);
    let mut out = Vec::with_capacity(IV_LEN + plaintext.len() + SIG_LEN);
    out.extend_from_slice(&iv);
    out.extend_from_slice(plaintext);
    let mut cipher = ChaCha20Legacy::new((&*key).into(), (&iv).into());
    cipher.apply_keystream(&mut out[IV_LEN..]);
    let hash = keccak256(&out);
    let sig = generate_signature(rng, &hash, &view.public, view.secret);
    out.extend_from_slice(&sig.to_bytes());
    out
}

/// `wallet2::decrypt_with_view_secret_key(ciphertext, authenticated = true)`.
pub fn decrypt(view: &ViewKey, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, SignerError> {
    if blob.len() < IV_LEN + SIG_LEN {
        return Err(SignerError::CiphertextTooShort);
    }
    let (body, sig) = blob.split_at(blob.len() - SIG_LEN);
    let sig = Signature::from_bytes(sig).ok_or(SignerError::BadAuth)?;
    let hash = keccak256(body);
    if !check_signature(&hash, &view.public, &sig) {
        return Err(SignerError::BadAuth);
    }
    let key = chacha_key_from_secret(&view.secret.to_bytes());
    let mut iv = [0u8; IV_LEN];
    iv.copy_from_slice(&body[..IV_LEN]);
    let mut plain = Zeroizing::new(body[IV_LEN..].to_vec());
    let mut cipher = ChaCha20Legacy::new((&*key).into(), (&iv).into());
    cipher.apply_keystream(&mut plain);
    Ok(plain)
}

pub fn decompress(bytes: &[u8; 32]) -> Option<EdwardsPoint> {
    CompressedEdwardsY(*bytes).decompress()
}

pub fn identity() -> EdwardsPoint {
    EdwardsPoint::identity()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CN_VECTORS: &str = include_str!("../fixtures/cn-slow-hash.txt");

    #[test]
    fn cryptonight_v0_matches_monero_vectors() {
        let mut n = 0;
        for line in CN_VECTORS.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let mut f = line.split_whitespace();
            let expected = hex::decode(f.next().unwrap()).unwrap();
            let input = hex::decode(f.next().unwrap()).unwrap();
            assert_eq!(&cryptonight_hash_v0(&input)[..], &expected[..], "vector {}", n);
            n += 1;
        }
        assert_eq!(n, 4);
    }

    #[test]
    fn signature_roundtrip_and_tamper() {
        let mut rng = rand_core::OsRng;
        let sec = Scalar::random(&mut rng);
        let public = &sec * ED25519_BASEPOINT_TABLE;
        let h = keccak256(b"hello");
        let sig = generate_signature(&mut rng, &h, &public, &sec);
        assert!(check_signature(&h, &public, &sig));
        let h2 = keccak256(b"hellp");
        assert!(!check_signature(&h2, &public, &sig));
        let parsed = Signature::from_bytes(&sig.to_bytes()).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let mut rng = rand_core::OsRng;
        let sec = Scalar::random(&mut rng);
        let view = ViewKey::new(&sec);
        let blob = encrypt(&mut rng, &view, b"Monero cold signing");
        assert_eq!(blob.len(), 8 + 19 + 64);
        let plain = decrypt(&view, &blob).unwrap();
        assert_eq!(&plain[..], b"Monero cold signing");
        let mut bad = blob.clone();
        bad[10] ^= 1;
        assert!(matches!(decrypt(&view, &bad), Err(SignerError::BadAuth)));
        let other = Scalar::random(&mut rng);
        assert!(decrypt(&ViewKey::new(&other), &blob).is_err());
    }
}
