# Monero SeedSigner research spike

Can a stateless, air-gapped SeedSigner-class device (Raspberry Pi Zero 1.3, ARMv6,
512 MB, no wireless) act as a Monero cold signer for an unmodified Feather Wallet?
This repository holds the numbers, the code that produced them, and the fixtures.

- `SPEC.md`: the task as written before the work started.
- `REPORT.md`: results, findings, decision record, FCMP++ note.
- `xmr-signer/`: Rust workspace.
  - `crates/libmonero-signer`: the signer core (wallet2 cold-signing payloads in,
    key images or signed transactions out, seed only in memory).
  - `crates/xmr-keys`: 25-word mnemonic decoding and key derivation.
  - `crates/xmr-signer-cli`: command-line front end used for the file round trip.
  - `crates/spike-bench`: benchmark harness (per-phase timings, peak RSS, QR frames).
  - `crates/xmr-gate`: the day-one gate (monero-oxide key images on ARMv6).
  - `fcmp-bench/`: FCMP++ spend-authorization proof cost on the device (own workspace).
  - `FORMATS.md`: byte-level specification of the four payloads, from source.
  - `PI_RUN.md`: how to reproduce every device number, and the bench card setup.
  - `fixtures/`: committed stagenet payloads with provenance notes.
  - `scripts/`: cross-build, card flashing, output fan-out, fixture generation.
- `backups/`: file-level backup of the SeedSigner card that was repurposed.

Everything is stagenet. The test wallet's private view key is in the report on
purpose (the fixtures are encrypted to it); the spend key and seed are not in the
repository.
