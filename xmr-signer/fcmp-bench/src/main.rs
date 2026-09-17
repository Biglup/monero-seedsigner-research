//! fcmp-bench: measures what a cold signer computes per input under FCMP++:
//! rerandomize the output, open the input tuple, and prove spend
//! authorization and linkability (SA/L). The membership proof and range
//! proof are the hot wallet's job and are not measured here.
//!
//!   fcmp-bench [--iters 20] [--inputs 1,2,16]

use std::time::Instant;

use ciphersuite::group::{ff::Field, Group};
use dalek_ff_group::{EdwardsPoint, Scalar};
use monero_fcmp_plus_plus::sal::{OpenedInputTuple, RerandomizedOutput, SpendAuthAndLinkability};
use monero_fcmp_plus_plus::Output;
use monero_ed25519::CompressedPoint;

fn peak_rss() -> (i64, &'static str) {
    let mut ru = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, ru.as_mut_ptr()) };
    if rc != 0 {
        return (-1, "unknown");
    }
    let ru = unsafe { ru.assume_init() };
    (ru.ru_maxrss as i64, if cfg!(target_os = "macos") { "bytes" } else { "kilobytes" })
}

fn stats(mut v: Vec<u128>) -> (u128, u128, u128, u128) {
    v.sort_unstable();
    (v[0], v[v.len() / 2], v.iter().sum::<u128>() / v.len() as u128, v[v.len() - 1])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut iters = 20usize;
    let mut sizes = vec![1usize, 2, 16];
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--iters" => { iters = args[i + 1].parse().unwrap(); i += 2; }
            "--inputs" => { sizes = args[i + 1].split(',').map(|s| s.parse().unwrap()).collect(); i += 2; }
            _ => { eprintln!("unknown arg"); std::process::exit(2); }
        }
    }
    let mut rng = rand_core::OsRng;
    let t_gen = EdwardsPoint(CompressedPoint::T.decompress().unwrap().into());
    println!("fcmp-bench (monero-oxide fcmp++ branch 31c26d96)");
    println!("target: {}", env!("TARGET_TRIPLE"));
    println!("iterations: {} measured after 1 warmup", iters);
    println!("{:<8} {:<12} {:>10} {:>10} {:>10} {:>10}", "inputs", "phase", "min_us", "median_us", "mean_us", "max_us");
    let mut result = format!("RESULT target={} iters={}", env!("TARGET_TRIPLE"), iters);

    // Synthetic spent outputs: O = xG + yT, random I and C, as in the crate's own test.
    let owned: Vec<(Scalar, Scalar, Output)> = (0..16)
        .map(|_| {
            let x = Scalar::random(&mut rng);
            let y = Scalar::random(&mut rng);
            let o = (EdwardsPoint::generator() * x) + (t_gen * y);
            (x, y, Output::new(o, EdwardsPoint::random(&mut rng), EdwardsPoint::random(&mut rng)).unwrap())
        })
        .collect();

    let mut proof_bytes = 0usize;
    for &n in &sizes {
        let mut t_rerand = vec![];
        let mut t_prove = vec![];
        for it in 0..=iters {
            let t = Instant::now();
            let openings: Vec<OpenedInputTuple> = owned[..n]
                .iter()
                .map(|(x, y, o)| {
                    let r = RerandomizedOutput::new(&mut rng, o.clone());
                    OpenedInputTuple::open(&r, x, y).expect("open")
                })
                .collect();
            let a = t.elapsed().as_micros();
            let t = Instant::now();
            let mut size = 0;
            for op in &openings {
                let (_l, proof) = SpendAuthAndLinkability::prove(&mut rng, [7u8; 32], op);
                let mut v = Vec::new();
                proof.write(&mut v).unwrap();
                size += v.len();
            }
            let b = t.elapsed().as_micros();
            if it > 0 {
                t_rerand.push(a);
                t_prove.push(b);
            }
            proof_bytes = size;
        }
        for (name, v) in [("rerandomize", t_rerand), ("sal_prove", t_prove)] {
            let (mn, md, me, mx) = stats(v);
            println!("{:<8} {:<12} {:>10} {:>10} {:>10} {:>10}", n, name, mn, md, me, mx);
            result += &format!(" in{}.{}_median_us={}", n, name, md);
        }
        println!("{:<8} sal proof bytes total: {} ({} per input)", n, proof_bytes, proof_bytes / n);
    }
    let (rss, unit) = peak_rss();
    println!("peak rss: {} {}", rss, unit);
    println!("{} sal_proof_bytes_per_input={} peak_rss_raw={} peak_rss_unit={}", result, proof_bytes / sizes.last().copied().unwrap_or(1), rss, unit);
}
