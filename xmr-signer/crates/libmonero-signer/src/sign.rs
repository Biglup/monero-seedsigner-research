//! Cold-side transaction construction and signing, mirroring
//! `wallet2::sign_tx` -> `cryptonote::construct_tx_with_tx_key` ->
//! `rct::genRctSimple` (CLSAG + Bulletproof+), and the `xmr-txsigned` reply.
//!
//! The transaction is assembled from monero-oxide primitives: the hot wallet
//! fixed the ring members, destinations, change and (implicitly) the fee, so
//! the high-level fee-rate driven builder cannot be used.

use std::collections::HashMap;

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::edwards::EdwardsPoint;
use curve25519_dalek::scalar::Scalar;
use monero_wallet::address::{AddressType, MoneroAddress, Network};
use monero_wallet::ed25519::{Commitment, CompressedPoint, Point, Scalar as MScalar};
use monero_wallet::ringct::bulletproofs::Bulletproof;
use monero_wallet::ringct::clsag::{Clsag, ClsagContext, Decoys};
use monero_wallet::ringct::{EncryptedAmount, RctBase, RctProofs, RctPrunable};
use monero_wallet::transaction::{Input, Output, Timelock, Transaction, TransactionPrefix};
use rand_core::{CryptoRng, RngCore};
use zeroize::Zeroizing;

use crate::archive::{Reader, Writer};
use crate::crypt::{self, decompress, hash_to_scalar, keccak256, ViewKey};
use crate::keyimage::{hash_to_point, subaddress_scalar};
use crate::txset::{Destination, TxConstructionData, UnsignedTxSet};
use crate::{AccountKeys, SignerError};

pub const MAGIC: &[u8] = b"Monero signed tx set\x05";

/// Subaddress lookahead used when a spent output is not listed in the
/// unsigned set's transfer export (major, minor).
pub const DEFAULT_LOOKAHEAD: (u32, u32) = (5, 200);

// ---------------------------------------------------------------- derivations

fn derivation(secret: &Scalar, public: &EdwardsPoint) -> [u8; 32] {
    (secret * public).mul_by_cofactor().compress().to_bytes()
}

fn derivation_scalar(d: &[u8; 32], index: u64) -> Scalar {
    let mut w = Writer::new();
    w.bytes(d);
    w.varint(index);
    hash_to_scalar(&w.out)
}

fn view_tag(d: &[u8; 32], index: u64) -> u8 {
    let mut w = Writer::new();
    w.bytes(b"view_tag");
    w.bytes(d);
    w.varint(index);
    keccak256(&w.out)[0]
}

fn commitment_mask(amount_key: &Scalar) -> Scalar {
    let mut w = Writer::new();
    w.bytes(b"commitment_mask");
    w.bytes(&amount_key.to_bytes());
    hash_to_scalar(&w.out)
}

fn encrypt_amount(amount_key: &Scalar, amount: u64) -> [u8; 8] {
    let mut w = Writer::new();
    w.bytes(b"amount");
    w.bytes(&amount_key.to_bytes());
    let h = keccak256(&w.out);
    let mut m = [0u8; 8];
    m.copy_from_slice(&h[..8]);
    (amount ^ u64::from_le_bytes(m)).to_le_bytes()
}

fn payment_id_xor(tx_key: &Scalar, view_public: &EdwardsPoint) -> [u8; 8] {
    let d = derivation(tx_key, view_public);
    let mut data = [0u8; 33];
    data[..32].copy_from_slice(&d);
    data[32] = 0x8d;
    let h = keccak256(&data);
    let mut m = [0u8; 8];
    m.copy_from_slice(&h[..8]);
    m
}

// ---------------------------------------------------------------- one-time keys

struct SpentKey {
    secret: Zeroizing<Scalar>,
    key_image: EdwardsPoint,
}

/// Rebuilds the subaddress spend-key table lazily, keyed by compressed key.
struct SubaddressTable<'a> {
    keys: &'a AccountKeys,
    spend_public: EdwardsPoint,
    known: HashMap<[u8; 32], (u32, u32)>,
    expanded: bool,
    lookahead: (u32, u32),
}

impl<'a> SubaddressTable<'a> {
    fn new(keys: &'a AccountKeys, set: &UnsignedTxSet, lookahead: (u32, u32)) -> Self {
        let spend_public = &*keys.spend * ED25519_BASEPOINT_TABLE;
        let mut known = HashMap::new();
        known.insert(spend_public.compress().to_bytes(), (0, 0));
        for t in &set.new_transfers {
            let sub = subaddress_scalar(&keys.view, t.subaddr_major, t.subaddr_minor);
            let d = spend_public + &sub * ED25519_BASEPOINT_TABLE;
            known.insert(d.compress().to_bytes(), (t.subaddr_major, t.subaddr_minor));
        }
        SubaddressTable { keys, spend_public, known, expanded: false, lookahead }
    }

    fn lookup(&mut self, spend_key: &[u8; 32]) -> Option<(u32, u32)> {
        if let Some(i) = self.known.get(spend_key) {
            return Some(*i);
        }
        if !self.expanded {
            self.expanded = true;
            for major in 0..self.lookahead.0 {
                for minor in 0..self.lookahead.1 {
                    let sub = subaddress_scalar(&self.keys.view, major, minor);
                    let d = self.spend_public + &sub * ED25519_BASEPOINT_TABLE;
                    self.known.entry(d.compress().to_bytes()).or_insert((major, minor));
                }
            }
            return self.known.get(spend_key).copied();
        }
        None
    }
}

fn spent_key(table: &mut SubaddressTable, out_key: &[u8; 32], tx_pubkey: &[u8; 32], additional: &[[u8; 32]], index: u64, which: usize) -> Result<SpentKey, SignerError> {
    let out_point = decompress(out_key).ok_or(SignerError::Malformed("spent output key not a point"))?;
    let mut candidates = vec![*tx_pubkey];
    if let Some(k) = additional.get(index as usize) {
        candidates.push(*k);
    }
    for tx_pub in candidates {
        let Some(p) = decompress(&tx_pub) else { continue };
        let d = derivation(&table.keys.view, &p);
        let s = derivation_scalar(&d, index);
        let spend_candidate = (out_point - &s * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        let Some((major, minor)) = table.lookup(&spend_candidate) else { continue };
        let sub = subaddress_scalar(&table.keys.view, major, minor);
        let secret = Zeroizing::new(s + &*table.keys.spend + sub);
        if &*secret * ED25519_BASEPOINT_TABLE != out_point {
            return Err(SignerError::KeyMismatch(which));
        }
        let key_image = &*secret * hash_to_point(out_key);
        return Ok(SpentKey { secret, key_image });
    }
    Err(SignerError::KeyMismatch(which))
}

// ---------------------------------------------------------------- tx_extra

#[derive(Clone)]
struct ExtraField {
    tag: u8,
    body: Vec<u8>,
}

fn parse_extra(extra: &[u8]) -> Result<Vec<ExtraField>, SignerError> {
    let mut r = Reader::new(extra);
    let mut fields = Vec::new();
    while !r.is_empty() {
        let tag = r.u8()?;
        let body = match tag {
            0x00 => {
                // padding: zero bytes to the end
                let n = r.remaining();
                r.bytes(n)?.to_vec()
            }
            0x01 => r.array32()?.to_vec(),
            0x02 | 0x03 | 0xde => {
                let n = r.count()?;
                r.bytes(n)?.to_vec()
            }
            0x04 => {
                let n = r.count()?;
                r.bytes(n * 32)?.to_vec()
            }
            _ => return Err(SignerError::Malformed("unknown tx_extra field")),
        };
        fields.push(ExtraField { tag, body });
    }
    Ok(fields)
}

fn write_extra(fields: &[ExtraField]) -> Vec<u8> {
    let mut sorted: Vec<&ExtraField> = fields.iter().collect();
    sorted.sort_by_key(|f| f.tag);
    let mut w = Writer::new();
    for f in sorted {
        w.u8(f.tag);
        match f.tag {
            0x00 | 0x01 => w.bytes(&f.body),
            0x04 => {
                w.varint((f.body.len() / 32) as u64);
                w.bytes(&f.body);
            }
            _ => {
                w.varint(f.body.len() as u64);
                w.bytes(&f.body);
            }
        }
    }
    w.out
}

/// `get_destination_view_key_pub`: the view key used to encrypt the short payment id.
fn destination_view_key(dests: &[Destination], change: &Destination) -> Option<[u8; 32]> {
    let mut found: Option<&Destination> = None;
    for d in dests {
        if d.amount == 0 || d.same_address(change) {
            continue;
        }
        if let Some(f) = found {
            if f.same_address(d) {
                continue;
            }
            return None;
        }
        found = Some(d);
    }
    match found {
        Some(d) => Some(d.view_public),
        None => Some(change.view_public),
    }
}

// ---------------------------------------------------------------- public API

/// What a transaction does, recomputed from the parsed construction data.
#[derive(Clone, Debug)]
pub struct TxSummary {
    pub inputs: usize,
    pub ring_size: usize,
    pub fee: u64,
    pub change: u64,
    /// (address, amount) for every non-change destination.
    pub payments: Vec<(String, u64)>,
}

impl TxSummary {
    pub fn total_out(&self) -> u64 {
        self.payments.iter().map(|p| p.1).sum()
    }
}

pub fn destination_address(network: Network, d: &Destination) -> String {
    let spend = CompressedPoint::from(d.spend_public).decompress();
    let view = CompressedPoint::from(d.view_public).decompress();
    match (spend, view) {
        (Some(s), Some(v)) => {
            let kind = if d.is_subaddress { AddressType::Subaddress } else { AddressType::Legacy };
            MoneroAddress::new(network, kind, s, v).to_string()
        }
        _ => "<invalid address>".to_string(),
    }
}

pub fn summarize(network: Network, tx: &TxConstructionData) -> TxSummary {
    let fee = tx.sources.iter().map(|s| s.amount).sum::<u64>().saturating_sub(tx.splitted_dsts.iter().map(|d| d.amount).sum::<u64>());
    let change = tx.splitted_dsts.iter().filter(|d| d.same_address(&tx.change)).map(|d| d.amount).sum();
    let payments = tx.splitted_dsts.iter().filter(|d| !d.same_address(&tx.change)).map(|d| (destination_address(network, d), d.amount)).collect();
    TxSummary { inputs: tx.sources.len(), ring_size: tx.sources.first().map(|s| s.ring.len()).unwrap_or(0), fee, change, payments }
}

pub struct SignedTx {
    pub tx: Transaction,
    pub fee: u64,
    pub key_images: Vec<[u8; 32]>,
    /// Output public key -> key image for the change outputs we own.
    pub own_output_key_images: Vec<([u8; 32], [u8; 32])>,
}

fn construct_and_sign(rng: &mut (impl RngCore + CryptoRng), keys: &AccountKeys, table: &mut SubaddressTable, sd: &TxConstructionData) -> Result<SignedTx, SignerError> {
    if sd.sources.is_empty() {
        return Err(SignerError::Malformed("empty sources"));
    }
    if sd.unlock_time != 0 {
        return Err(SignerError::Malformed("non-zero unlock time"));
    }
    if !sd.use_rct || sd.rct_config.range_proof_type != 3 || sd.rct_config.bp_version != 4 || !sd.use_view_tags {
        return Err(SignerError::Malformed("only RingCT with padded Bulletproof+ (CLSAG) and view tags is supported"));
    }
    for s in &sd.sources {
        if !s.rct {
            return Err(SignerError::Malformed("pre-RingCT input"));
        }
    }

    // 1. inputs: recover one-time keys, key images, sort by key image descending
    let mut ins: Vec<(SpentKey, usize)> = Vec::with_capacity(sd.sources.len());
    for (i, s) in sd.sources.iter().enumerate() {
        let real = &s.ring[s.real_output as usize];
        let k = spent_key(table, &real.key, &s.real_out_tx_key, &s.real_out_additional_tx_keys, s.real_output_in_tx_index, i)?;
        ins.push((k, i));
    }
    ins.sort_by(|a, b| b.0.key_image.compress().to_bytes().cmp(&a.0.key_image.compress().to_bytes()));

    let mut inputs = Vec::with_capacity(ins.len());
    let mut clsag_inputs = Vec::with_capacity(ins.len());
    let mut key_images = Vec::with_capacity(ins.len());
    for (k, si) in &ins {
        let s = &sd.sources[*si];
        let mut offsets = Vec::with_capacity(s.ring.len());
        let mut prev = 0u64;
        for (j, m) in s.ring.iter().enumerate() {
            offsets.push(if j == 0 { m.global_index } else { m.global_index.checked_sub(prev).ok_or(SignerError::Malformed("ring not sorted"))? });
            prev = m.global_index;
        }
        let ki = k.key_image.compress().to_bytes();
        key_images.push(ki);
        inputs.push(Input::ToKey { amount: None, key_offsets: offsets.clone(), key_image: CompressedPoint::from(ki) });
        let mut ring = Vec::with_capacity(s.ring.len());
        for m in &s.ring {
            let key = CompressedPoint::from(m.key).decompress().ok_or(SignerError::Malformed("ring member key"))?;
            let com = CompressedPoint::from(m.commitment).decompress().ok_or(SignerError::Malformed("ring member commitment"))?;
            ring.push([key, com]);
        }
        let decoys = Decoys::new(offsets, s.real_output as u8, ring).ok_or(SignerError::Malformed("invalid decoy set"))?;
        let mask = Scalar::from_canonical_bytes(s.mask).into_option().ok_or(SignerError::Malformed("input mask not canonical"))?;
        let ctx = ClsagContext::new(decoys, Commitment::new(MScalar::from(mask), s.amount)).map_err(|_| SignerError::Malformed("clsag context"))?;
        clsag_inputs.push((Zeroizing::new(MScalar::from(*k.secret)), ctx));
    }

    // 2. destinations: shuffle, classify
    let mut dests = sd.splitted_dsts.clone();
    for i in (1..dests.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        dests.swap(i, j);
    }
    let (mut num_std, mut num_sub) = (0usize, 0usize);
    let mut single_sub: Option<&Destination> = None;
    let mut seen: Vec<&Destination> = Vec::new();
    for d in &dests {
        if d.same_address(&sd.change) || seen.iter().any(|s| s.same_address(d)) {
            continue;
        }
        seen.push(d);
        if d.is_subaddress {
            num_sub += 1;
            single_sub = Some(d);
        } else {
            num_std += 1;
        }
    }
    let need_additional = num_sub > 0 && (num_std > 0 || num_sub > 1);

    // 3. transaction keys
    let tx_key = Zeroizing::new(Scalar::random(rng));
    let additional: Vec<Zeroizing<Scalar>> = if need_additional { dests.iter().map(|_| Zeroizing::new(Scalar::random(rng))).collect() } else { vec![] };
    let txkey_pub = if num_std == 0 && num_sub == 1 {
        let sp = decompress(&single_sub.expect("counted").spend_public).ok_or(SignerError::Malformed("dest spend key"))?;
        &*tx_key * sp
    } else {
        &*tx_key * ED25519_BASEPOINT_TABLE
    };

    // 4. tx_extra: keep the hot wallet's fields, (re)encrypt the short payment id, add keys
    let mut fields: Vec<ExtraField> = parse_extra(&sd.extra)?.into_iter().filter(|f| f.tag != 0x01 && f.tag != 0x04).collect();
    let mut add_dummy = true;
    if let Some(pos) = fields.iter().position(|f| f.tag == 0x02) {
        let nonce = fields[pos].body.clone();
        if nonce.len() == 9 && nonce[0] == 0x01 {
            let vk = destination_view_key(&dests, &sd.change).ok_or(SignerError::Malformed("payment id with several destinations"))?;
            let vk = decompress(&vk).ok_or(SignerError::Malformed("dest view key"))?;
            let x = payment_id_xor(&tx_key, &vk);
            let mut body = vec![0x01];
            body.extend(nonce[1..9].iter().zip(x).map(|(a, b)| a ^ b));
            fields[pos].body = body;
            add_dummy = false;
        } else if nonce.len() == 33 && nonce[0] == 0x00 {
            add_dummy = false;
        }
    }
    if dests.len() > 2 {
        add_dummy = false;
    }
    if add_dummy {
        if let Some(vk) = destination_view_key(&dests, &sd.change).and_then(|v| decompress(&v)) {
            let x = payment_id_xor(&tx_key, &vk);
            let mut body = vec![0x01];
            body.extend_from_slice(&x);
            fields.push(ExtraField { tag: 0x02, body });
        }
    }
    fields.push(ExtraField { tag: 0x01, body: txkey_pub.compress().to_bytes().to_vec() });

    // 5. outputs
    let mut outputs = Vec::with_capacity(dests.len());
    let mut commitments = Vec::with_capacity(dests.len());
    let mut encrypted = Vec::with_capacity(dests.len());
    let mut additional_pubs: Vec<u8> = Vec::new();
    let mut own_outputs: Vec<([u8; 32], [u8; 32])> = Vec::new();
    let mut mask_sum = Scalar::ZERO;
    for (i, d) in dests.iter().enumerate() {
        let dest_spend = decompress(&d.spend_public).ok_or(SignerError::Malformed("dest spend key"))?;
        let dest_view = decompress(&d.view_public).ok_or(SignerError::Malformed("dest view key"))?;
        let is_change = d.same_address(&sd.change);
        let der = if is_change {
            derivation(&keys.view, &txkey_pub)
        } else {
            let key = if d.is_subaddress && need_additional { &additional[i] } else { &tx_key };
            derivation(key, &dest_view)
        };
        if need_additional {
            let p = if d.is_subaddress { &*additional[i] * dest_spend } else { &*additional[i] * ED25519_BASEPOINT_TABLE };
            additional_pubs.extend_from_slice(&p.compress().to_bytes());
        }
        let amount_key = derivation_scalar(&der, i as u64);
        let out_key = &amount_key * ED25519_BASEPOINT_TABLE + dest_spend;
        let vt = view_tag(&der, i as u64);
        let mask = commitment_mask(&amount_key);
        mask_sum += mask;
        let c = Commitment::new(MScalar::from(mask), d.amount);
        commitments.push(c);
        encrypted.push(EncryptedAmount::Compact { amount: encrypt_amount(&amount_key, d.amount) });
        outputs.push(Output { key: Point::from(out_key).compress(), amount: None, view_tag: Some(vt) });
        if is_change {
            // key image of our own change output, for the hot wallet
            let sub = subaddress_scalar(&keys.view, 0, 0);
            let _ = sub;
            let major_minor = table.lookup(&sd.change.spend_public);
            if let Some((major, minor)) = major_minor {
                let sub = subaddress_scalar(&keys.view, major, minor);
                let secret = Zeroizing::new(amount_key + &*keys.spend + sub);
                let ok = &*secret * ED25519_BASEPOINT_TABLE;
                let ki = &*secret * hash_to_point(&ok.compress().to_bytes());
                own_outputs.push((ok.compress().to_bytes(), ki.compress().to_bytes()));
            }
        }
    }
    if need_additional {
        fields.push(ExtraField { tag: 0x04, body: additional_pubs });
    }
    let extra = write_extra(&fields);

    let in_sum: u64 = sd.sources.iter().map(|s| s.amount).sum();
    let out_sum: u64 = dests.iter().map(|d| d.amount).sum();
    let fee = in_sum.checked_sub(out_sum).ok_or(SignerError::Malformed("outputs exceed inputs"))?;

    // 6. range proof and transaction skeleton
    let bulletproof = Bulletproof::prove_plus(rng, commitments.clone()).map_err(|_| SignerError::Malformed("bulletproof+"))?;
    let mut tx = Transaction::V2 {
        prefix: TransactionPrefix { additional_timelock: Timelock::None, inputs, outputs, extra },
        proofs: Some(RctProofs {
            base: RctBase { fee, encrypted_amounts: encrypted, pseudo_outs: vec![], commitments: commitments.iter().map(|c| c.commit().compress()).collect() },
            prunable: RctPrunable::Clsag { bulletproof, clsags: vec![], pseudo_outs: vec![] },
        }),
    };

    // 7. CLSAGs
    let msg = tx.signature_hash().ok_or(SignerError::Malformed("no signature hash"))?;
    let sigs = Clsag::sign(rng, clsag_inputs, MScalar::from(mask_sum), msg).map_err(|_| SignerError::Malformed("clsag sign"))?;
    let Transaction::V2 { proofs: Some(RctProofs { prunable: RctPrunable::Clsag { ref mut clsags, ref mut pseudo_outs, .. }, .. }), .. } = tx else { unreachable!() };
    for (c, p) in sigs {
        clsags.push(c);
        pseudo_outs.push(p.compress());
    }
    Ok(SignedTx { tx, fee, key_images, own_output_key_images: own_outputs })
}

pub struct SignResult {
    pub blob: Vec<u8>,
    pub txs: Vec<SignedTx>,
    pub summaries: Vec<TxSummary>,
}

/// Signs every transaction in the set and builds the `xmr-txsigned` payload.
pub fn sign(rng: &mut (impl RngCore + CryptoRng), keys: &AccountKeys, network: Network, set: &UnsignedTxSet, lookahead: (u32, u32)) -> Result<SignResult, SignerError> {
    let mut table = SubaddressTable::new(keys, set, lookahead);
    let mut txs = Vec::with_capacity(set.txes.len());
    let mut summaries = Vec::with_capacity(set.txes.len());
    for sd in &set.txes {
        summaries.push(summarize(network, sd));
        txs.push(construct_and_sign(rng, keys, &mut table, sd)?);
    }

    // key images for every exported transfer, as wallet2's sign_tx does
    let mut tx_key_images: Vec<([u8; 32], [u8; 32])> = Vec::new();
    for t in &txs {
        tx_key_images.extend(t.own_output_key_images.iter().copied());
    }
    for (i, o) in set.new_transfers.iter().enumerate() {
        let k = crate::keyimage::one_time_key(keys, o, i)?;
        tx_key_images.push((o.pubkey, k.key_image.compress().to_bytes()));
    }
    tx_key_images.sort();
    tx_key_images.dedup();

    let mut w = Writer::new();
    w.varint(0); // signed_tx_set version
    w.varint(txs.len() as u64);
    for (t, sd) in txs.iter().zip(&set.txes) {
        w.varint(1); // pending_tx version
        w.bytes(&t.tx.serialize());
        w.u64_le(0); // dust
        w.u64_le(t.fee);
        w.bool(false); // dust_added_to_fee
        w.bytes(&sd.raw_change);
        w.bytes(&sd.raw_selected_transfers);
        let mut kis = String::new();
        for ki in &t.key_images {
            kis.push_str(&hex(ki));
            kis.push(' ');
        }
        w.varint(kis.len() as u64);
        w.bytes(kis.as_bytes());
        let mut one = [0u8; 32];
        one[0] = 1;
        w.bytes(&one); // tx_key = identity: never returned to the hot wallet
        w.varint(0); // additional_tx_keys
        w.bytes(&sd.raw_dests);
        w.bytes(&sd.raw);
        w.varint(0); // multisig_sigs
        w.bytes(&[0u8; 32]); // multisig_tx_key_entropy
    }
    w.varint(0); // key_images (unused by the string path)
    w.varint(tx_key_images.len() as u64);
    for (pk, ki) in &tx_key_images {
        w.varint(2);
        w.bytes(pk);
        w.bytes(ki);
    }
    let view = ViewKey::new(&keys.view);
    let mut blob = MAGIC.to_vec();
    blob.extend_from_slice(&crypt::encrypt(rng, &view, &w.out));
    Ok(SignResult { blob, txs, summaries })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}
