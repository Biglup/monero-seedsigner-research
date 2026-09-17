//! `xmr-output` payload: wallet2's outputs export.
//!
//! "Monero output export\x04" || encrypt( spend_pub[32] || view_pub[32] ||
//!   binary_archive( tuple<u64 offset, u64 total, vector<exported_transfer_details>> ) )

use curve25519_dalek::edwards::EdwardsPoint;

use crate::archive::Reader;
use crate::crypt::{self, ViewKey};
use crate::SignerError;

pub const MAGIC: &[u8] = b"Monero output export\x04";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedOutput {
    pub pubkey: [u8; 32],
    pub internal_output_index: u64,
    pub global_output_index: u64,
    pub tx_pubkey: [u8; 32],
    pub flags: u8,
    pub amount: u64,
    pub additional_tx_keys: Vec<[u8; 32]>,
    pub subaddr_major: u32,
    pub subaddr_minor: u32,
}

impl ExportedOutput {
    pub fn spent(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn frozen(&self) -> bool {
        self.flags & 2 != 0
    }
    pub fn rct(&self) -> bool {
        self.flags & 4 != 0
    }
    pub fn key_image_known(&self) -> bool {
        self.flags & 8 != 0
    }
    pub fn key_image_request(&self) -> bool {
        self.flags & 16 != 0
    }

    pub fn read(r: &mut Reader) -> Result<Self, SignerError> {
        let version = r.varint()?;
        if version < 1 {
            return Err(SignerError::Malformed("exported_transfer_details version < 1"));
        }
        let pubkey = r.array32()?;
        let internal_output_index = r.varint()?;
        let global_output_index = r.varint()?;
        let tx_pubkey = r.array32()?;
        let flags = r.u8()?;
        let amount = r.varint()?;
        let n = r.count()?;
        let mut additional_tx_keys = Vec::with_capacity(n);
        for _ in 0..n {
            additional_tx_keys.push(r.array32()?);
        }
        let subaddr_major = r.varint()? as u32;
        let subaddr_minor = r.varint()? as u32;
        Ok(ExportedOutput {
            pubkey,
            internal_output_index,
            global_output_index,
            tx_pubkey,
            flags,
            amount,
            additional_tx_keys,
            subaddr_major,
            subaddr_minor,
        })
    }
}

#[derive(Clone, Debug)]
pub struct OutputsExport {
    pub spend_public: [u8; 32],
    pub view_public: [u8; 32],
    pub offset: u64,
    pub total: u64,
    pub outputs: Vec<ExportedOutput>,
}

impl OutputsExport {
    /// Parses and authenticates a full `xmr-output` payload for the account
    /// identified by `view` and `spend_public`.
    pub fn parse(view: &ViewKey, spend_public: &EdwardsPoint, blob: &[u8]) -> Result<Self, SignerError> {
        if blob.len() < MAGIC.len() || &blob[..MAGIC.len()] != MAGIC {
            return Err(SignerError::BadMagic("outputs export"));
        }
        let plain = crypt::decrypt(view, &blob[MAGIC.len()..])?;
        let mut r = Reader::new(&plain);
        let spend_pub = r.array32()?;
        let view_pub = r.array32()?;
        if spend_pub != spend_public.compress().to_bytes() || view_pub != view.public.compress().to_bytes() {
            return Err(SignerError::WrongAccount);
        }
        // tuple<u64, u64, vector<...>>: array of 3, unsigned elements as varints
        if r.varint()? != 3 {
            return Err(SignerError::Malformed("outputs tuple arity"));
        }
        let offset = r.varint()?;
        let total = r.varint()?;
        let n = r.count()?;
        let mut outputs = Vec::with_capacity(n);
        for _ in 0..n {
            outputs.push(ExportedOutput::read(&mut r)?);
        }
        if !r.is_empty() {
            return Err(SignerError::Malformed("trailing bytes after outputs"));
        }
        Ok(OutputsExport { spend_public: spend_pub, view_public: view_pub, offset, total, outputs })
    }
}
