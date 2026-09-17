//! spike-bench: runs the cold-signing pipeline over the fixture snapshots and
//! prints per-phase timings (min / median / mean / max over N measured
//! iterations after one warmup) plus peak RSS and animated-QR frame counts.
//!
//!   spike-bench <fixtures-dir> [--iters 20] [--snapshots snap50,snap200,snap500]
//!
//! Seed: XMR_SEED or XMR_SEED_FILE (the stagenet test wallet). Fixtures per
//! snapshot: outputs.bin, unsigned_2in.bin, unsigned_16in.bin.
//! Phases: parse_outputs (decrypt + parse), key_images (derive, sign, build
//! reply), parse_2in, sign_2in, parse_16in, sign_16in (construct + BP+ +
//! CLSAG + encrypted reply). All timings in microseconds.

use std::path::Path;
use std::time::Instant;

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use libmonero_signer::crypt::ViewKey;
use libmonero_signer::{keyimage, outputs::OutputsExport, sign, txset::UnsignedTxSet, AccountKeys};
use monero_wallet::address::Network;
use zeroize::Zeroizing;

fn peak_rss() -> (i64, &'static str) {
    let mut ru = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage fills the struct on success.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, ru.as_mut_ptr()) };
    if rc != 0 {
        return (-1, "unknown");
    }
    let ru = unsafe { ru.assume_init() };
    (ru.ru_maxrss as i64, if cfg!(target_os = "macos") { "bytes" } else { "kilobytes" })
}

fn stats(mut v: Vec<u128>) -> (u128, u128, u128, u128) {
    v.sort_unstable();
    let n = v.len() as u128;
    (v[0], v[v.len() / 2], v.iter().sum::<u128>() / n, v[v.len() - 1])
}

/// UR fountain encoder sequence length for a payload of `payload` bytes at
/// `frag` bytes per fragment: ceil(cbor_len / frag), where the CBOR byte
/// string header adds 1 + (1 | 2 | 3 | 5) bytes.
fn ur_frames(payload: usize, frag: usize) -> usize {
    let hdr = if payload < 24 { 1 } else if payload < 256 { 2 } else if payload < 65536 { 3 } else { 5 };
    (payload + hdr).div_ceil(frag)
}

fn load_keys() -> AccountKeys {
    let seed = match std::env::var("XMR_SEED") {
        Ok(s) => Zeroizing::new(s),
        Err(_) => Zeroizing::new(std::fs::read_to_string(std::env::var("XMR_SEED_FILE").expect("set XMR_SEED or XMR_SEED_FILE")).expect("seed file")),
    };
    let words: Vec<&str> = seed.split_whitespace().collect();
    AccountKeys::from_mnemonic(&words).expect("seed")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).map(String::as_str).unwrap_or("fixtures");
    let mut iters = 20usize;
    let mut snapshots = vec!["snap50".to_string(), "snap200".to_string(), "snap500".to_string()];
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--iters" => { iters = args[i + 1].parse().expect("iters"); i += 2; }
            "--snapshots" => { snapshots = args[i + 1].split(',').map(String::from).collect(); i += 2; }
            _ => { eprintln!("unknown arg {}", args[i]); std::process::exit(2); }
        }
    }
    let keys = load_keys();
    let view = ViewKey::new(&keys.view);
    let spend_pub = &*keys.spend * ED25519_BASEPOINT_TABLE;
    let mut rng = rand_core::OsRng;

    println!("spike-bench (libmonero-signer {})", env!("CARGO_PKG_VERSION"));
    println!("target: {}", env!("TARGET_TRIPLE"));
    println!("iterations: {} measured after 1 warmup, per phase", iters);
    println!("fixtures: {}", dir);
    println!();
    println!("{:<8} {:<14} {:>10} {:>10} {:>10} {:>10}", "snapshot", "phase", "min_us", "median_us", "mean_us", "max_us");

    let mut result = format!("RESULT target={} iters={}", env!("TARGET_TRIPLE"), iters);
    let mut sizes = String::new();

    for snap in &snapshots {
        let d = Path::new(dir).join(snap);
        let Ok(outputs_blob) = std::fs::read(d.join("outputs.bin")) else {
            eprintln!("skipping {}: no outputs.bin", snap);
            continue;
        };
        // outputs -> key images
        let mut t_parse = vec![];
        let mut t_ki = vec![];
        let mut n_outputs = 0;
        let mut ki_len = 0;
        for it in 0..=iters {
            let t = Instant::now();
            let export = OutputsExport::parse(&view, &spend_pub, &outputs_blob).expect("outputs");
            let a = t.elapsed().as_micros();
            let t = Instant::now();
            let images = keyimage::sign_key_images(&mut rng, &keys, &export.outputs).expect("key images");
            let blob = keyimage::export_blob(&mut rng, &keys, export.offset as u32, &images);
            let b = t.elapsed().as_micros();
            if it > 0 {
                t_parse.push(a);
                t_ki.push(b);
            }
            n_outputs = export.outputs.len();
            ki_len = blob.len();
        }
        for (name, v) in [("parse_outputs", t_parse), ("key_images", t_ki)] {
            let (mn, md, me, mx) = stats(v);
            println!("{:<8} {:<14} {:>10} {:>10} {:>10} {:>10}", snap, name, mn, md, me, mx);
            result += &format!(" {}.{}_median_us={}", snap, name, md);
        }
        sizes += &format!("{} wallet_outputs={} outputs_export_bytes={} keyimage_export_bytes={}", snap, n_outputs, outputs_blob.len(), ki_len);
        for frag in [30usize, 120, 150] {
            sizes += &format!(" outputs_frames@{}={} keyimages_frames@{}={}", frag, ur_frames(outputs_blob.len(), frag), frag, ur_frames(ki_len, frag));
        }
        sizes += "\n";

        for n_in in ["2in", "16in"] {
            let Ok(unsigned) = std::fs::read(d.join(format!("unsigned_{}.bin", n_in))) else {
                eprintln!("skipping {} {}: no fixture", snap, n_in);
                continue;
            };
            let mut t_parse = vec![];
            let mut t_sign = vec![];
            let mut signed_len = 0;
            let mut inputs = 0;
            for it in 0..=iters {
                let t = Instant::now();
                let set = UnsignedTxSet::parse(&view, &spend_pub, &unsigned).expect("unsigned set");
                let a = t.elapsed().as_micros();
                let t = Instant::now();
                let res = sign::sign(&mut rng, &keys, Network::Stagenet, &set, sign::DEFAULT_LOOKAHEAD).expect("sign");
                let b = t.elapsed().as_micros();
                if it > 0 {
                    t_parse.push(a);
                    t_sign.push(b);
                }
                signed_len = res.blob.len();
                inputs = set.txes[0].sources.len();
            }
            for (name, v) in [(format!("parse_{}", n_in), t_parse), (format!("sign_{}", n_in), t_sign)] {
                let (mn, md, me, mx) = stats(v);
                println!("{:<8} {:<14} {:>10} {:>10} {:>10} {:>10}", snap, name, mn, md, me, mx);
                result += &format!(" {}.{}_median_us={}", snap, name, md);
            }
            sizes += &format!("{} tx_inputs={} unsigned_tx_bytes={} signed_tx_bytes={}", snap, inputs, unsigned.len(), signed_len);
            for frag in [30usize, 120, 150] {
                sizes += &format!(" unsigned_frames@{}={} signed_frames@{}={}", frag, ur_frames(unsigned.len(), frag), frag, ur_frames(signed_len, frag));
            }
            sizes += "\n";
        }
    }
    let (rss, unit) = peak_rss();
    println!();
    print!("{}", sizes);
    println!("peak rss: {} {}", rss, unit);
    println!("{} peak_rss_raw={} peak_rss_unit={}", result, rss, unit);
}
