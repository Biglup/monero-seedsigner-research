//! Stateless Monero cold-signer core for the SeedSigner spike.
//!
//! Input and output formats are wallet2's cold-signing blobs as used by
//! Feather's offline transaction signing wizard, wrapped in the Keystone
//! `xmr-*` UR types. See ../../FORMATS.md for the byte-level specification
//! and the provenance of every constant here.
//!
//! Nothing in this crate touches the filesystem. Secrets live in `Zeroizing`
//! wrappers and are dropped as soon as the operation completes.

pub mod archive;
pub mod crypt;
pub mod keyimage;
pub mod outputs;
pub mod sign;
pub mod txset;

pub use xmr_keys::AccountKeys;

#[derive(Debug, thiserror::Error)]
pub enum SignerError {
    #[error("bad magic: not a {0} blob")]
    BadMagic(&'static str),
    #[error("ciphertext too short")]
    CiphertextTooShort,
    #[error("authentication failed: signature over the ciphertext does not verify with the view key")]
    BadAuth,
    #[error("blob is for a different account (public keys do not match)")]
    WrongAccount,
    #[error("malformed data: {0}")]
    Malformed(&'static str),
    #[error("output {0}: derived one-time public key does not match the exported output key")]
    KeyMismatch(usize),
}
