//! Prints the stagenet (or given network) primary address for a 25-word seed read from stdin.
use monero_wallet::address::Network;
use std::io::Read;

fn main() {
    let net = match std::env::args().nth(1).as_deref() {
        Some("mainnet") => Network::Mainnet,
        Some("testnet") => Network::Testnet,
        _ => Network::Stagenet,
    };
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    let words: Vec<&str> = s.split_whitespace().collect();
    let keys = xmr_keys::AccountKeys::from_mnemonic(&words).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1)
    });
    println!("{}", keys.primary_address(net));
}
