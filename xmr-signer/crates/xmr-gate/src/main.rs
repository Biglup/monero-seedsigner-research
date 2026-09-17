//! Day-one gate for the Monero SeedSigner spike.
//!
//! Proves that monero-oxide's wallet crate (monero-wallet) cross-compiles to
//! arm-unknown-linux-musleabihf and computes correct Monero key images on a
//! Raspberry Pi Zero 1.3. Correctness is checked against the upstream
//! generate_key_image vectors from monero-project/monero (see fixtures/).
//! Nothing is read from disk and nothing is written; the vectors are embedded.

use std::time::Instant;

use curve25519_dalek::scalar::Scalar as DalekScalar;
use monero_wallet::address::Network;
use monero_wallet::ed25519::{CompressedPoint, Point, Scalar};
use zeroize::Zeroizing;

const VECTORS: &str = include_str!("../fixtures/generate_key_image.txt");

fn hex32(s: &str) -> [u8; 32] {
    assert_eq!(s.len(), 64, "expected 64 hex chars, got {}", s.len());
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex");
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Exactly the derivation monero-wallet uses in `SignableTransaction::sign`:
/// key_image = x * biased_hash(P), where x is the output's private key and P
/// its public key.
fn key_image(sec: &Zeroizing<DalekScalar>, pubkey: [u8; 32]) -> [u8; 32] {
    let hp: curve25519_dalek::EdwardsPoint = Point::biased_hash(pubkey).into();
    Point::from(**sec * hp).compress().to_bytes()
}

fn peak_rss() -> (i64, &'static str) {
    let mut ru = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage writes a fully initialised rusage on success.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, ru.as_mut_ptr()) };
    if rc != 0 {
        return (-1, "unknown");
    }
    let ru = unsafe { ru.assume_init() };
    let unit = if cfg!(target_os = "macos") { "bytes" } else { "kilobytes" };
    (ru.ru_maxrss as i64, unit)
}

fn main() {
    let iters: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("iters must be an integer"))
        .unwrap_or(1);

    let vectors: Vec<([u8; 32], Zeroizing<DalekScalar>, [u8; 32])> = VECTORS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut f = l.split_whitespace();
            let pubkey = hex32(f.next().unwrap());
            let sec = DalekScalar::from_canonical_bytes(hex32(f.next().unwrap()))
                .expect("secret key must be a canonical scalar");
            let expected = hex32(f.next().unwrap());
            (pubkey, Zeroizing::new(sec), expected)
        })
        .collect();

    println!("xmr-gate (monero-oxide key image gate)");
    println!("target: {}", env!("TARGET_TRIPLE"));
    println!("vectors: {} (monero-project/monero tests/crypto/tests.txt, generate_key_image)", vectors.len());
    println!("iterations over the vector set: {}", iters);

    // Prove the wallet crate itself links, not just the curve crate: build a
    // ViewPair from the first vector and derive a stagenet address.
    let (p0, s0, _) = &vectors[0];
    let spend = CompressedPoint::from(*p0).decompress().expect("vector pubkey decompresses");
    let view = Zeroizing::new(Scalar::from(**s0));
    let pair = monero_wallet::ViewPair::new(spend, view).expect("ViewPair");
    println!("wallet crate check: stagenet address from vector 0 = {}", pair.legacy_address(Network::Stagenet));

    let mut ok = 0usize;
    let mut bad = 0usize;
    let mut per_ki_us: Vec<u128> = Vec::with_capacity(vectors.len() * iters);
    let start = Instant::now();
    for _ in 0..iters {
        for (pubkey, sec, expected) in &vectors {
            let t = Instant::now();
            let ki = key_image(sec, *pubkey);
            per_ki_us.push(t.elapsed().as_micros());
            if &ki == expected {
                ok += 1;
            } else {
                bad += 1;
                println!("MISMATCH pub={} got={} expected={}", hex(pubkey), hex(&ki), hex(expected));
            }
        }
    }
    let total = start.elapsed();
    per_ki_us.sort_unstable();
    let median = per_ki_us[per_ki_us.len() / 2];
    let min = per_ki_us[0];
    let max = per_ki_us[per_ki_us.len() - 1];
    let (rss, unit) = peak_rss();

    println!();
    println!("key images computed: {}  correct: {}  wrong: {}", ok + bad, ok, bad);
    println!("per key image us: min {}  median {}  max {}", min, median, max);
    println!("total ms: {}", total.as_millis());
    println!("peak rss: {} {}", rss, unit);
    println!(
        "RESULT target={} vectors={} iters={} ok={} bad={} ki_min_us={} ki_median_us={} ki_max_us={} total_ms={} peak_rss_raw={} peak_rss_unit={}",
        env!("TARGET_TRIPLE"), vectors.len(), iters, ok, bad, min, median, max, total.as_millis(), rss, unit
    );
    if bad != 0 {
        std::process::exit(1);
    }
}
