//! Prints the primary address and the private VIEW key (hex) for a seed on
//! stdin, for setting up a view-only hot wallet. Never prints the spend key.
use monero_wallet::address::Network;
use std::io::Read;

fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    let words: Vec<&str> = s.split_whitespace().collect();
    let keys = xmr_keys::AccountKeys::from_mnemonic(&words).expect("seed");
    println!("address:  {}", keys.primary_address(Network::Stagenet));
    let v: [u8; 32] = keys.view.to_bytes();
    println!("view key: {}", v.iter().map(|b| format!("{:02x}", b)).collect::<String>());
}
