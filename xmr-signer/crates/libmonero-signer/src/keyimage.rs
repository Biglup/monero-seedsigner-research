//! Key image generation for exported outputs and the `xmr-keyimage` reply.
//!
//! Mirrors `cryptonote::generate_key_image_helper` and wallet2's
//! `export_key_images_for_outputs_from_str`:
//!   derivation D = 8 * (view_secret * tx_pubkey)
//!   scalar     s = Hs(D || varint(output_index))
//!   spend pub  for (major, minor): B, or B + Hs("SubAddr\0" || view_secret || major || minor) * G
//!   check      out_key - s * G == that spend pub (first with tx_pubkey, then additional_tx_keys[index])
//!   x          = s + b (+ subaddress scalar)     and x * G must equal out_key
//!   key image  = x * hash_to_point(out_key)
//!   signature  = one-member CryptoNote ring signature over prefix_hash = key_image
//! Reply blob: "Monero key image export\x03" || encrypt( offset_u32_le || spend_pub || view_pub || N x (key_image || sig) )

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::edwards::EdwardsPoint;
use curve25519_dalek::scalar::Scalar;
use monero_wallet::ed25519::Point;
use rand_core::{CryptoRng, RngCore};
use zeroize::Zeroizing;

use crate::archive::Writer;
use crate::crypt::{self, decompress, hash_to_scalar, ViewKey};
use crate::outputs::ExportedOutput;
use crate::{AccountKeys, SignerError};

pub const MAGIC: &[u8] = b"Monero key image export\x03";

pub fn hash_to_point(bytes: &[u8; 32]) -> EdwardsPoint {
    Point::biased_hash(*bytes).into()
}

fn key_derivation(view_secret: &Scalar, tx_pubkey: &[u8; 32]) -> Option<[u8; 32]> {
    let p = decompress(tx_pubkey)?;
    Some((view_secret * p).mul_by_cofactor().compress().to_bytes())
}

fn derivation_to_scalar(derivation: &[u8; 32], index: u64) -> Scalar {
    let mut w = Writer::new();
    w.bytes(derivation);
    w.varint(index);
    hash_to_scalar(&w.out)
}

/// Hs("SubAddr\0" || view_secret || major || minor), zero for (0, 0).
pub fn subaddress_scalar(view_secret: &Scalar, major: u32, minor: u32) -> Scalar {
    if major == 0 && minor == 0 {
        return Scalar::ZERO;
    }
    let mut w = Writer::new();
    w.bytes(b"SubAddr\0");
    w.bytes(&view_secret.to_bytes());
    w.u32_le(major);
    w.u32_le(minor);
    hash_to_scalar(&w.out)
}

pub struct OneTimeKey {
    pub secret: Zeroizing<Scalar>,
    pub public: EdwardsPoint,
    pub key_image: EdwardsPoint,
}

/// Recovers the one-time secret key of an exported output and its key image.
pub fn one_time_key(keys: &AccountKeys, out: &ExportedOutput, index_in_export: usize) -> Result<OneTimeKey, SignerError> {
    let out_key = decompress(&out.pubkey).ok_or(SignerError::Malformed("output key not a point"))?;
    let spend_pub = &*keys.spend * ED25519_BASEPOINT_TABLE;
    let sub = subaddress_scalar(&keys.view, out.subaddr_major, out.subaddr_minor);
    let expected_spend = spend_pub + &sub * ED25519_BASEPOINT_TABLE;

    let mut candidates: Vec<[u8; 32]> = vec![out.tx_pubkey];
    if let Some(k) = out.additional_tx_keys.get(out.internal_output_index as usize) {
        candidates.push(*k);
    }
    for tx_pub in candidates {
        let Some(d) = key_derivation(&keys.view, &tx_pub) else { continue };
        let s = derivation_to_scalar(&d, out.internal_output_index);
        if out_key - &s * ED25519_BASEPOINT_TABLE != expected_spend {
            continue;
        }
        let secret = Zeroizing::new(s + &*keys.spend + sub);
        let public = &*secret * ED25519_BASEPOINT_TABLE;
        if public != out_key {
            return Err(SignerError::KeyMismatch(index_in_export));
        }
        let key_image = &*secret * hash_to_point(&out.pubkey);
        return Ok(OneTimeKey { secret, public, key_image });
    }
    Err(SignerError::KeyMismatch(index_in_export))
}

/// `crypto::generate_ring_signature` with a single ring member: proves
/// knowledge of x for P = x*G and I = x*Hp(P), over `prefix_hash`.
pub fn ring_signature_1(
    rng: &mut (impl RngCore + CryptoRng),
    prefix_hash: &[u8; 32],
    public: &EdwardsPoint,
    key_image: &EdwardsPoint,
    secret: &Scalar,
) -> crypt::Signature {
    let hp = hash_to_point(&public.compress().to_bytes());
    loop {
        let a = Zeroizing::new(Scalar::random(rng));
        let l = (&*a * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        let r = (&*a * hp).compress().to_bytes();
        let mut buf = [0u8; 96];
        buf[..32].copy_from_slice(prefix_hash);
        buf[32..64].copy_from_slice(&l);
        buf[64..].copy_from_slice(&r);
        let c = hash_to_scalar(&buf);
        let _ = key_image;
        let rr = &*a - c * secret;
        if c == Scalar::ZERO || rr == Scalar::ZERO {
            continue;
        }
        return crypt::Signature { c, r: rr };
    }
}

/// `crypto::check_ring_signature` for one member.
pub fn check_ring_signature_1(prefix_hash: &[u8; 32], public: &EdwardsPoint, key_image: &EdwardsPoint, sig: &crypt::Signature) -> bool {
    let hp = hash_to_point(&public.compress().to_bytes());
    let l = sig.c * public + &sig.r * ED25519_BASEPOINT_TABLE;
    let r = sig.c * key_image + sig.r * hp;
    let mut buf = [0u8; 96];
    buf[..32].copy_from_slice(prefix_hash);
    buf[32..64].copy_from_slice(&l.compress().to_bytes());
    buf[64..].copy_from_slice(&r.compress().to_bytes());
    hash_to_scalar(&buf) == sig.c
}

pub struct SignedKeyImage {
    pub key_image: [u8; 32],
    pub signature: crypt::Signature,
}

/// Computes signed key images for every exported output, in order.
pub fn sign_key_images(rng: &mut (impl RngCore + CryptoRng), keys: &AccountKeys, outputs: &[ExportedOutput]) -> Result<Vec<SignedKeyImage>, SignerError> {
    let mut out = Vec::with_capacity(outputs.len());
    for (i, o) in outputs.iter().enumerate() {
        let k = one_time_key(keys, o, i)?;
        let ki = k.key_image.compress().to_bytes();
        let signature = ring_signature_1(rng, &ki, &k.public, &k.key_image, &k.secret);
        out.push(SignedKeyImage { key_image: ki, signature });
    }
    Ok(out)
}

/// Builds the `xmr-keyimage` payload.
pub fn export_blob(rng: &mut (impl RngCore + CryptoRng), keys: &AccountKeys, offset: u32, images: &[SignedKeyImage]) -> Vec<u8> {
    let view = ViewKey::new(&keys.view);
    let spend_pub = (&*keys.spend * ED25519_BASEPOINT_TABLE).compress().to_bytes();
    let mut w = Writer::new();
    w.u32_le(offset);
    w.bytes(&spend_pub);
    w.bytes(&view.public.compress().to_bytes());
    for i in images {
        w.bytes(&i.key_image);
        w.bytes(&i.signature.to_bytes());
    }
    let mut blob = MAGIC.to_vec();
    blob.extend_from_slice(&crypt::encrypt(rng, &view, &w.out));
    blob
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_signature_1_roundtrip() {
        let mut rng = rand_core::OsRng;
        let x = Scalar::random(&mut rng);
        let p = &x * ED25519_BASEPOINT_TABLE;
        let i = x * hash_to_point(&p.compress().to_bytes());
        let h = i.compress().to_bytes();
        let sig = ring_signature_1(&mut rng, &h, &p, &i, &x);
        assert!(check_ring_signature_1(&h, &p, &i, &sig));
        let wrong = Scalar::random(&mut rng) * hash_to_point(&p.compress().to_bytes());
        assert!(!check_ring_signature_1(&h, &p, &wrong, &sig));
    }
}
