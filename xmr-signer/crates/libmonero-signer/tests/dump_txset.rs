use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use libmonero_signer::crypt::{self, ViewKey};
use libmonero_signer::AccountKeys;

#[test]
fn dump_unsigned_txset_plaintext() {
    let Ok(seed) = std::env::var("XMR_TEST_SEED") else { return };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/dev-vectors");
    let Ok(blob) = std::fs::read(dir.join("unsigned-txset-1.bin")) else { return };
    let words: Vec<&str> = seed.split_whitespace().collect();
    let keys = AccountKeys::from_mnemonic(&words).unwrap();
    let view = ViewKey::new(&keys.view);
    let _ = &*keys.spend * ED25519_BASEPOINT_TABLE;
    let plain = crypt::decrypt(&view, &blob[b"Monero unsigned tx set\x05".len()..]).unwrap();
    std::fs::write(dir.join("unsigned-txset-1.plain"), &plain[..]).unwrap();
    println!("plain {} bytes", plain.len());
    for (i, chunk) in plain[..plain.len().min(420)].chunks(32).enumerate() {
        println!("{:04x}: {}", i * 32, chunk.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" "));
    }
}
