//! End-to-end check against blobs produced by monero-wallet-rpc 0.18.5.1 on
//! the spike's stagenet test wallet. Runs only when XMR_TEST_SEED is set and
//! the local (gitignored) development vectors exist:
//!   local/dev-vectors/outputs-all.bin      export_outputs {all: true}
//!   local/dev-vectors/key-images-all.json  export_key_images {all: true}

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use libmonero_signer::crypt::{self, ViewKey};
use libmonero_signer::keyimage;
use libmonero_signer::outputs::OutputsExport;
use libmonero_signer::AccountKeys;

fn vectors_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/dev-vectors")
}

#[test]
fn rpc_outputs_export_to_key_images() {
    let Ok(seed) = std::env::var("XMR_TEST_SEED") else {
        eprintln!("XMR_TEST_SEED not set, skipping");
        return;
    };
    let dir = vectors_dir();
    let Ok(blob) = std::fs::read(dir.join("outputs-all.bin")) else {
        eprintln!("no dev vectors, skipping");
        return;
    };
    let words: Vec<&str> = seed.split_whitespace().collect();
    let keys = AccountKeys::from_mnemonic(&words).unwrap();
    let view = ViewKey::new(&keys.view);
    let spend_pub = &*keys.spend * ED25519_BASEPOINT_TABLE;

    let export = OutputsExport::parse(&view, &spend_pub, &blob).expect("parse outputs export");
    println!("outputs export: offset {} total {} outputs {} blob {} bytes", export.offset, export.total, export.outputs.len(), blob.len());
    assert_eq!(export.outputs.len() as u64, export.total - export.offset);
    let mut rng = rand_core::OsRng;
    let signed = keyimage::sign_key_images(&mut rng, &keys, &export.outputs).expect("key images");

    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("key-images-all.json")).unwrap()).unwrap();
    let expected = json["signed_key_images"].as_array().unwrap();
    assert_eq!(expected.len(), signed.len(), "key image count");
    for (i, (ours, theirs)) in signed.iter().zip(expected).enumerate() {
        assert_eq!(hex::encode(ours.key_image), theirs["key_image"].as_str().unwrap(), "key image {}", i);
        // wallet2's signature must verify with our one-member ring verifier
        let sig = crypt::Signature::from_bytes(&hex::decode(theirs["signature"].as_str().unwrap()).unwrap()).unwrap();
        let k = keyimage::one_time_key(&keys, &export.outputs[i], i).unwrap();
        assert!(keyimage::check_ring_signature_1(&ours.key_image, &k.public, &k.key_image, &sig), "wallet2 signature {} verifies", i);
        assert!(keyimage::check_ring_signature_1(&ours.key_image, &k.public, &k.key_image, &ours.signature), "our signature {} verifies", i);
    }
    println!("all {} key images match wallet2 and both signature sets verify", signed.len());

    // Our reply blob decrypts and lays out as wallet2 expects.
    let reply = keyimage::export_blob(&mut rng, &keys, export.offset as u32, &signed);
    assert!(reply.starts_with(keyimage::MAGIC));
    let plain = crypt::decrypt(&view, &reply[keyimage::MAGIC.len()..]).unwrap();
    assert_eq!(plain.len(), 4 + 64 + signed.len() * 96);
    assert_eq!(&plain[4..36], &spend_pub.compress().to_bytes());
    assert_eq!(&plain[36..68], &view.public.compress().to_bytes());
    println!("key image reply blob: {} bytes", reply.len());
    std::fs::write(dir.join("key-images-ours.bin"), &reply).unwrap();
    let per_out: Vec<usize> = export.outputs.iter().map(|o| 32 + 32 + 1 + 8 + o.additional_tx_keys.len() * 32).collect();
    println!("additional tx keys per output: min {} max {}", export.outputs.iter().map(|o| o.additional_tx_keys.len()).min().unwrap(), export.outputs.iter().map(|o| o.additional_tx_keys.len()).max().unwrap());
    let _ = per_out;
}

#[test]
fn rpc_unsigned_txset_to_signed_txset() {
    use libmonero_signer::sign;
    use libmonero_signer::txset::UnsignedTxSet;
    use monero_wallet::address::Network;

    let Ok(seed) = std::env::var("XMR_TEST_SEED") else { return };
    let dir = vectors_dir();
    let Ok(blob) = std::fs::read(dir.join("unsigned-txset-1.bin")) else { return };
    let words: Vec<&str> = seed.split_whitespace().collect();
    let keys = AccountKeys::from_mnemonic(&words).unwrap();
    let view = ViewKey::new(&keys.view);
    let spend_pub = &*keys.spend * ED25519_BASEPOINT_TABLE;

    let set = UnsignedTxSet::parse(&view, &spend_pub, &blob).expect("parse unsigned tx set");
    println!("unsigned set: {} tx, transfers {}..{} ({} exported), blob {} bytes", set.txes.len(), set.transfers_offset, set.transfers_total, set.new_transfers.len(), blob.len());
    for (i, t) in set.txes.iter().enumerate() {
        let s = sign::summarize(Network::Stagenet, t);
        println!("tx {}: {} inputs, ring {}, fee {} pXMR, change {} pXMR, payments {:?}, extra {} bytes, rct_config {:?}, view_tags {}", i, s.inputs, s.ring_size, s.fee, s.change, s.payments, t.extra.len(), t.rct_config, t.use_view_tags);
    }
    let mut rng = rand_core::OsRng;
    let t = std::time::Instant::now();
    let res = sign::sign(&mut rng, &keys, Network::Stagenet, &set, sign::DEFAULT_LOOKAHEAD).expect("sign");
    println!("signed in {} ms, blob {} bytes, tx hash {}", t.elapsed().as_millis(), res.blob.len(), hex::encode(res.txs[0].tx.hash()));
    println!("tx serialized {} bytes, key images {:?}", res.txs[0].tx.serialize().len(), res.txs[0].key_images.iter().map(hex::encode).collect::<Vec<_>>());
    std::fs::write(dir.join("signed-txset-1.bin"), &res.blob).unwrap();

    // our own blob must decrypt and start with the expected structure
    let plain = crypt::decrypt(&view, &res.blob[sign::MAGIC.len()..]).unwrap();
    assert_eq!(plain[0], 0);
    assert_eq!(plain[1], set.txes.len() as u8);
}
