//! `xmr-txunsigned` payload: wallet2's unsigned transaction set.
//!
//! "Monero unsigned tx set\x05" || encrypt( binary_archive(unsigned_tx_set) )
//! See FORMATS.md section 5. Raw byte ranges of the sub-structures that the
//! signed reply must echo back verbatim (change_dts, selected_transfers,
//! dests, the whole tx_construction_data) are kept as slices so no writer
//! for them is needed.

use curve25519_dalek::edwards::EdwardsPoint;

use crate::archive::Reader;
use crate::crypt::{self, ViewKey};
use crate::outputs::ExportedOutput;
use crate::SignerError;

pub const MAGIC: &[u8] = b"Monero unsigned tx set\x05";

#[derive(Clone, Debug)]
pub struct RingMember {
    pub global_index: u64,
    pub key: [u8; 32],
    pub commitment: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct TxSource {
    pub ring: Vec<RingMember>,
    pub real_output: u64,
    pub real_out_tx_key: [u8; 32],
    pub real_out_additional_tx_keys: Vec<[u8; 32]>,
    pub real_output_in_tx_index: u64,
    pub amount: u64,
    pub rct: bool,
    pub mask: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub original: Vec<u8>,
    pub amount: u64,
    pub spend_public: [u8; 32],
    pub view_public: [u8; 32],
    pub is_subaddress: bool,
    pub is_integrated: bool,
}

impl Destination {
    pub fn same_address(&self, other: &Destination) -> bool {
        self.spend_public == other.spend_public && self.view_public == other.view_public
    }
}

#[derive(Clone, Debug)]
pub struct RctConfig {
    pub range_proof_type: u64,
    pub bp_version: u64,
}

#[derive(Clone, Debug)]
pub struct TxConstructionData {
    pub sources: Vec<TxSource>,
    pub change: Destination,
    pub splitted_dsts: Vec<Destination>,
    pub selected_transfers: Vec<u64>,
    pub extra: Vec<u8>,
    pub unlock_time: u64,
    pub use_rct: bool,
    pub use_view_tags: bool,
    pub rct_config: RctConfig,
    pub dests: Vec<Destination>,
    pub subaddr_account: u32,
    pub subaddr_indices: Vec<u32>,
    /// Raw encodings echoed into the signed reply.
    pub raw: Vec<u8>,
    pub raw_change: Vec<u8>,
    pub raw_selected_transfers: Vec<u8>,
    pub raw_dests: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct UnsignedTxSet {
    pub txes: Vec<TxConstructionData>,
    pub transfers_offset: u64,
    pub transfers_total: u64,
    pub new_transfers: Vec<ExportedOutput>,
}

fn read_destination(r: &mut Reader) -> Result<Destination, SignerError> {
    let n = r.count()?;
    let original = r.bytes(n)?.to_vec();
    let amount = r.varint()?;
    let spend_public = r.array32()?;
    let view_public = r.array32()?;
    let is_subaddress = r.bool()?;
    let is_integrated = r.bool()?;
    Ok(Destination { original, amount, spend_public, view_public, is_subaddress, is_integrated })
}

fn read_destinations(r: &mut Reader) -> Result<Vec<Destination>, SignerError> {
    let n = r.count()?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(read_destination(r)?);
    }
    Ok(v)
}

fn read_source(r: &mut Reader) -> Result<TxSource, SignerError> {
    let n = r.count()?;
    let mut ring = Vec::with_capacity(n);
    for _ in 0..n {
        if r.varint()? != 2 {
            return Err(SignerError::Malformed("ring member pair arity"));
        }
        let global_index = r.varint()?;
        let key = r.array32()?;
        let commitment = r.array32()?;
        ring.push(RingMember { global_index, key, commitment });
    }
    let real_output = r.u64_le()?;
    let real_out_tx_key = r.array32()?;
    let na = r.count()?;
    let mut real_out_additional_tx_keys = Vec::with_capacity(na);
    for _ in 0..na {
        real_out_additional_tx_keys.push(r.array32()?);
    }
    let real_output_in_tx_index = r.u64_le()?;
    let amount = r.u64_le()?;
    let rct = r.bool()?;
    let mask = r.array32()?;
    let _multisig_klrki = r.bytes(128)?;
    if real_output as usize >= ring.len() {
        return Err(SignerError::Malformed("real_output outside ring"));
    }
    Ok(TxSource { ring, real_output, real_out_tx_key, real_out_additional_tx_keys, real_output_in_tx_index, amount, rct, mask })
}

impl TxConstructionData {
    pub fn read(r: &mut Reader, data: &[u8]) -> Result<Self, SignerError> {
        let start = r.position();
        let n = r.count()?;
        let mut sources = Vec::with_capacity(n);
        for _ in 0..n {
            sources.push(read_source(r)?);
        }
        let c0 = r.position();
        let change = read_destination(r)?;
        let raw_change = data[c0..r.position()].to_vec();
        let splitted_dsts = read_destinations(r)?;
        let s0 = r.position();
        let n = r.count()?;
        let mut selected_transfers = Vec::with_capacity(n);
        for _ in 0..n {
            selected_transfers.push(r.varint()?);
        }
        let raw_selected_transfers = data[s0..r.position()].to_vec();
        let n = r.count()?;
        let extra = r.bytes(n)?.to_vec();
        let unlock_time = r.u64_le()?;
        let flags = r.u8()?;
        if r.varint()? != 0 {
            return Err(SignerError::Malformed("rct_config version"));
        }
        let rct_config = RctConfig { range_proof_type: r.varint()?, bp_version: r.varint()? };
        let d0 = r.position();
        let dests = read_destinations(r)?;
        let raw_dests = data[d0..r.position()].to_vec();
        let subaddr_account = r.u32_le()?;
        let n = r.count()?;
        let mut subaddr_indices = Vec::with_capacity(n);
        for _ in 0..n {
            subaddr_indices.push(r.varint()? as u32);
        }
        let raw = data[start..r.position()].to_vec();
        Ok(TxConstructionData {
            sources,
            change,
            splitted_dsts,
            selected_transfers,
            extra,
            unlock_time,
            use_rct: flags & 1 != 0,
            use_view_tags: flags & 2 != 0,
            rct_config,
            dests,
            subaddr_account,
            subaddr_indices,
            raw,
            raw_change,
            raw_selected_transfers,
            raw_dests,
        })
    }
}

impl UnsignedTxSet {
    pub fn parse(view: &ViewKey, _spend_public: &EdwardsPoint, blob: &[u8]) -> Result<Self, SignerError> {
        if blob.len() < MAGIC.len() || &blob[..MAGIC.len()] != MAGIC {
            return Err(SignerError::BadMagic("unsigned tx set"));
        }
        let plain = crypt::decrypt(view, &blob[MAGIC.len()..])?;
        Self::parse_plain(&plain)
    }

    pub fn parse_plain(plain: &[u8]) -> Result<Self, SignerError> {
        let mut r = Reader::new(plain);
        let version = r.varint()?;
        if version != 2 {
            return Err(SignerError::Malformed("unsigned_tx_set version is not 2"));
        }
        let n = r.count()?;
        let mut txes = Vec::with_capacity(n);
        for _ in 0..n {
            txes.push(TxConstructionData::read(&mut r, plain)?);
        }
        if r.varint()? != 3 {
            return Err(SignerError::Malformed("new_transfers tuple arity"));
        }
        let transfers_offset = r.varint()?;
        let transfers_total = r.varint()?;
        let n = r.count()?;
        let mut new_transfers = Vec::with_capacity(n);
        for _ in 0..n {
            new_transfers.push(ExportedOutput::read(&mut r)?);
        }
        if !r.is_empty() {
            return Err(SignerError::Malformed("trailing bytes after unsigned tx set"));
        }
        Ok(UnsignedTxSet { txes, transfers_offset, transfers_total, new_transfers })
    }
}
