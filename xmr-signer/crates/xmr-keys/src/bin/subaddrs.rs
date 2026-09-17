//! Prints primary address and subaddresses (account 0, index 0..n) for a seed on stdin.
use monero_wallet::address::{Network, SubaddressIndex};
use monero_wallet::ed25519::Scalar;
use monero_wallet::ViewPair;
use std::io::Read;
use zeroize::Zeroizing;

fn main() {
    let n: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    let words: Vec<&str> = s.split_whitespace().collect();
    let keys = xmr_keys::AccountKeys::from_mnemonic(&words).expect("seed");
    let pair = ViewPair::new(keys.spend_public(), Zeroizing::new(Scalar::from(*keys.view))).expect("view pair");
    println!("0/0 {}", pair.legacy_address(Network::Stagenet));
    for i in 1..=n {
        let idx = SubaddressIndex::new(0, i).expect("index");
        println!("0/{} {}", i, pair.subaddress(Network::Stagenet, idx));
    }
}
