//! Monero legacy 25-word mnemonic decoding and account key derivation.
//!
//! Word list: monero-project/monero src/mnemonics/english.h (1626 words,
//! unique prefix length 3), copied to wordlists/english.txt.
//! Encoding: each group of 3 words encodes one little-endian u32 with
//! n = 1626: w1 + n * ((w2 - w1) mod n) + n^2 * ((w3 - w2) mod n).
//! Word 25 is a checksum: crc32 over the 3-letter prefixes of the 24 words,
//! index = crc32 % 24.
//! Keys: spend_sk = sc_reduce32(seed), view_sk = sc_reduce32(keccak256(spend_sk)).

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::scalar::Scalar;
use monero_wallet::address::{AddressType, MoneroAddress, Network};
use monero_wallet::ed25519::Point;
use sha3::{Digest, Keccak256};
use zeroize::{Zeroize, Zeroizing};

const WORDS: &str = include_str!("../wordlists/english.txt");
const PREFIX_LEN: usize = 3;

#[derive(Debug)]
pub enum SeedError {
    WordCount(usize),
    UnknownWord(String),
    Checksum,
}

impl core::fmt::Display for SeedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SeedError::WordCount(n) => write!(f, "expected 25 words, got {}", n),
            SeedError::UnknownWord(w) => write!(f, "unknown word: {}", w),
            SeedError::Checksum => write!(f, "checksum word does not match"),
        }
    }
}

fn wordlist() -> Vec<&'static str> {
    WORDS.lines().filter(|l| !l.is_empty()).collect()
}

fn find(list: &[&str], word: &str) -> Option<u32> {
    // Match on the unique prefix, as monero does, so truncated words also work.
    let p: String = word.chars().take(PREFIX_LEN).collect();
    list.iter().position(|w| w.chars().take(PREFIX_LEN).collect::<String>() == p).map(|i| i as u32)
}

/// Decodes a 25-word English mnemonic into the 32-byte seed (pre-reduction).
pub fn decode_mnemonic(words: &[&str]) -> Result<Zeroizing<[u8; 32]>, SeedError> {
    if words.len() != 25 {
        return Err(SeedError::WordCount(words.len()));
    }
    let list = wordlist();
    let n = list.len() as u32;
    let mut idx = Vec::with_capacity(25);
    for w in words {
        idx.push(find(&list, w).ok_or_else(|| SeedError::UnknownWord((*w).to_string()))?);
    }
    // checksum
    let mut h = crc32fast::Hasher::new();
    for w in &words[..24] {
        h.update(w.chars().take(PREFIX_LEN).collect::<String>().as_bytes());
    }
    let expected = (h.finalize() % 24) as usize;
    if list[idx[24] as usize].chars().take(PREFIX_LEN).collect::<String>()
        != words[expected].chars().take(PREFIX_LEN).collect::<String>()
    {
        return Err(SeedError::Checksum);
    }
    let mut seed = Zeroizing::new([0u8; 32]);
    for i in 0..8 {
        let (w1, w2, w3) = (idx[3 * i], idx[3 * i + 1], idx[3 * i + 2]);
        let x = w1 + n * ((n + w2 - w1) % n) + n * n * ((n + w3 - w2) % n);
        seed[4 * i..4 * i + 4].copy_from_slice(&x.to_le_bytes());
    }
    idx.zeroize();
    Ok(seed)
}

/// Spend and view secret keys.
pub struct AccountKeys {
    pub spend: Zeroizing<Scalar>,
    pub view: Zeroizing<Scalar>,
}

impl AccountKeys {
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let spend = Zeroizing::new(Scalar::from_bytes_mod_order(*seed));
        let h: [u8; 32] = Keccak256::digest(spend.to_bytes()).into();
        let view = Zeroizing::new(Scalar::from_bytes_mod_order(h));
        Self { spend, view }
    }

    pub fn from_mnemonic(words: &[&str]) -> Result<Self, SeedError> {
        let seed = decode_mnemonic(words)?;
        Ok(Self::from_seed(&seed))
    }

    pub fn spend_public(&self) -> Point {
        Point::from(&*self.spend * ED25519_BASEPOINT_TABLE)
    }

    pub fn view_public(&self) -> Point {
        Point::from(&*self.view * ED25519_BASEPOINT_TABLE)
    }

    pub fn primary_address(&self, network: Network) -> MoneroAddress {
        MoneroAddress::new(network, AddressType::Legacy, self.spend_public(), self.view_public())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The spike's stagenet test seed lives outside the repo (xmr-signer/local/,
    // gitignored). Set XMR_TEST_SEED="word1 ... word25" to run this check;
    // the derived address must match what Feather shows for the same seed.
    #[test]
    fn stagenet_seed_from_env_derives_a_standard_address() {
        let Ok(seed) = std::env::var("XMR_TEST_SEED") else { return };
        let words: Vec<&str> = seed.split_whitespace().collect();
        let keys = AccountKeys::from_mnemonic(&words).unwrap();
        let addr = keys.primary_address(Network::Stagenet).to_string();
        assert!(addr.starts_with('5'), "stagenet standard addresses start with 5, got {}", addr);
        assert_eq!(addr.len(), 95);
    }

    #[test]
    fn checksum_word_is_enforced() {
        let list = wordlist();
        // 24 distinct words; the checksum must pick one of them, so a word from
        // outside that set can never be a valid 25th word.
        let mut words: Vec<&str> = list[..24].to_vec();
        words.push(list[100]);
        assert!(matches!(decode_mnemonic(&words), Err(SeedError::Checksum)));
        // Fix the checksum and it decodes.
        let mut h = crc32fast::Hasher::new();
        for w in &words[..24] {
            h.update(w.chars().take(PREFIX_LEN).collect::<String>().as_bytes());
        }
        let ck = words[(h.finalize() % 24) as usize];
        words[24] = ck;
        assert!(decode_mnemonic(&words).is_ok());
    }

    #[test]
    fn word_count_is_enforced() {
        assert!(matches!(decode_mnemonic(&["abbey"; 24]), Err(SeedError::WordCount(24))));
    }
}
