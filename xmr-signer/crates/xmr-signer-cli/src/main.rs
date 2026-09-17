//! xmr-signer: stateless command-line cold signer for wallet2 payloads.
//!
//!   xmr-signer keyimages <outputs-export.bin> <key-images-out.bin>
//!   xmr-signer sign      <unsigned-tx-set.bin> <signed-tx-set-out.bin>
//!   xmr-signer show      <unsigned-tx-set.bin>
//!
//! The 25-word seed is read from the XMR_SEED environment variable or from
//! the file named by XMR_SEED_FILE. Network defaults to stagenet
//! (XMR_NETWORK=mainnet|stagenet|testnet). Nothing else is read or written.

use std::time::Instant;

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use libmonero_signer::crypt::ViewKey;
use libmonero_signer::{keyimage, outputs::OutputsExport, sign, txset::UnsignedTxSet, AccountKeys};
use monero_wallet::address::Network;
use zeroize::Zeroizing;

fn die(msg: &str) -> ! {
    eprintln!("error: {}", msg);
    std::process::exit(2)
}

fn load_keys() -> AccountKeys {
    let seed = match std::env::var("XMR_SEED") {
        Ok(s) => Zeroizing::new(s),
        Err(_) => match std::env::var("XMR_SEED_FILE") {
            Ok(f) => Zeroizing::new(std::fs::read_to_string(f).unwrap_or_else(|e| die(&format!("reading seed file: {}", e)))),
            Err(_) => die("set XMR_SEED or XMR_SEED_FILE"),
        },
    };
    let words: Vec<&str> = seed.split_whitespace().collect();
    AccountKeys::from_mnemonic(&words).unwrap_or_else(|e| die(&format!("seed: {}", e)))
}

fn network() -> Network {
    match std::env::var("XMR_NETWORK").as_deref() {
        Ok("mainnet") => Network::Mainnet,
        Ok("testnet") => Network::Testnet,
        _ => Network::Stagenet,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        die("usage: xmr-signer keyimages <in> <out> | sign <in> <out> | show <in>");
    }
    let input = std::fs::read(&args[2]).unwrap_or_else(|e| die(&format!("reading {}: {}", args[2], e)));
    let keys = load_keys();
    let view = ViewKey::new(&keys.view);
    let spend_pub = &*keys.spend * ED25519_BASEPOINT_TABLE;
    let mut rng = rand_core::OsRng;
    match args[1].as_str() {
        "keyimages" => {
            let out_path = args.get(3).unwrap_or_else(|| die("missing output path"));
            let t = Instant::now();
            let export = OutputsExport::parse(&view, &spend_pub, &input).unwrap_or_else(|e| die(&e.to_string()));
            let t_parse = t.elapsed();
            let t = Instant::now();
            let images = keyimage::sign_key_images(&mut rng, &keys, &export.outputs).unwrap_or_else(|e| die(&e.to_string()));
            let blob = keyimage::export_blob(&mut rng, &keys, export.offset as u32, &images);
            let t_ki = t.elapsed();
            std::fs::write(out_path, &blob).unwrap_or_else(|e| die(&format!("writing {}: {}", out_path, e)));
            eprintln!("outputs: {} (offset {}, total {}), input {} bytes, reply {} bytes, parse {} ms, key images {} ms", export.outputs.len(), export.offset, export.total, input.len(), blob.len(), t_parse.as_millis(), t_ki.as_millis());
        }
        "show" | "sign" => {
            let t = Instant::now();
            let set = UnsignedTxSet::parse(&view, &spend_pub, &input).unwrap_or_else(|e| die(&e.to_string()));
            let t_parse = t.elapsed();
            let net = network();
            for (i, tx) in set.txes.iter().enumerate() {
                let s = sign::summarize(net, tx);
                eprintln!("tx {}: inputs {} (ring {}), fee {:.12} XMR, change {:.12} XMR, total out {:.12} XMR", i, s.inputs, s.ring_size, s.fee as f64 / 1e12, s.change as f64 / 1e12, s.total_out() as f64 / 1e12);
                for (addr, amount) in &s.payments {
                    eprintln!("    -> {} {:.12} XMR", addr, *amount as f64 / 1e12);
                }
            }
            eprintln!("exported transfers in set: {}, input {} bytes, parse {} ms", set.new_transfers.len(), input.len(), t_parse.as_millis());
            if args[1] == "sign" {
                let out_path = args.get(3).unwrap_or_else(|| die("missing output path"));
                let t = Instant::now();
                let res = sign::sign(&mut rng, &keys, net, &set, sign::DEFAULT_LOOKAHEAD).unwrap_or_else(|e| die(&e.to_string()));
                let t_sign = t.elapsed();
                std::fs::write(out_path, &res.blob).unwrap_or_else(|e| die(&format!("writing {}: {}", out_path, e)));
                for tx in &res.txs {
                    eprintln!("signed tx {} ({} bytes)", tx.tx.hash().iter().map(|b| format!("{:02x}", b)).collect::<String>(), tx.tx.serialize().len());
                }
                eprintln!("reply {} bytes, sign {} ms", res.blob.len(), t_sign.as_millis());
            }
        }
        _ => die("unknown command"),
    }
}
