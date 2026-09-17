# Stage 2 notes: integrating the signer into the SeedSigner fork

Survey of cardano-seedsigner and cardano-seedsigner-os, 2026-09-17. File and line
references are to those repositories at that date.

## Native code pattern

- The Cardano flow calls native code through cffi in ABI mode: `ffi.dlopen` on
  `/usr/lib/libcardano-c.so`, byte buffers in via `ffi.from_buffer`, buffers out via
  out-pointer plus explicit unref, integer error codes plus a last-error accessor
  (binding source: cometa.py `src/cometa/_ffi.py`, `buffer.py`, `cbor/cbor_writer.py`,
  `errors.py`). The app imports `cometa` late, in a background import thread
  (`controller.py:91`).
- The device image is glibc, not musl: `BR2_TOOLCHAIN_BUILDROOT_GLIBC=y`,
  `BR2_arm1176jzf_s=y`, effectively `arm-unknown-linux-gnueabihf`. A static musl
  binary (what this spike builds) runs on it; a musl cdylib cannot be dlopen'ed into
  glibc CPython. Two options for the Monero flow:
  1. Ship `xmr-signer` as a static musl binary and call it with subprocess (new
     pattern for this codebase, zero Buildroot Rust work, seed passed via env).
  2. Build `libmonero-signer` as a gnueabihf cdylib with `cargo zigbuild` and load it
     with cffi like cometa (matches the existing pattern, needs a small C ABI).
  Either way the artifact is prebuilt and pinned by sha256 in
  `opt/external-packages/<name>/` and installed to `/usr/lib` or `/usr/bin`; the
  whole rootfs is an initramfs inside `zImage` on a 50 MB FAT partition, so binary
  size matters (the static CLI is 1.2 MB).

## UR transport

- Decoder allowlist: `models/decode_qr.py:19-26` (`UR_DECODER_QR_TYPES`); detectors
  are regexes built from the type strings; completed payload CBOR via
  `decoder.result_message().cbor` (`decode_qr.py:191-198`). The Keystone `xmr-*`
  payload is a bare CBOR byte string, so the flow decodes one `bstr`.
- Encoder: subclass `CardanoUrQrEncoder` pattern in `models/encode_qr.py:241-287`
  with `ur_type` set; fragment size from the density setting, 10 / 30 / 120 bytes.
  `QRDisplayScreen` animates at about 6 fps (`gui/screens/screen.py:866`), so 120
  bytes per fragment is about 720 bytes/s: key images for 51 outputs (5 KB) about 7 s,
  a signed 2-input tx (6.8 KB) about 10 s, a signed 16-input tx (35 KB) about 50 s.
- Camera scans at 480x480, 6 fps (`gui/screens/scan_screens.py:17-73`); the 2-minute
  screensaver is reset after a scan.

## Flow structure and seed handling

- Views chain via `Destination`; no Flow class. Scan-driven entry in
  `views/scan_views.py:106-152`; per-seed menu in `views/seed_views.py:431-487`;
  resume-after-seed-load hook in `views/seed_views.py:246-257`; controller flow
  constants `controller.py:129-142`. Pure signing helpers live in
  `helpers/cardano_signing.py`; mirror as `helpers/monero_signing.py`.
- Slow work runs synchronously under `LoadingScreenThread` (`screen.py:138-207`).
- Confirm screen pattern: overview rows then `CardanoTxSignScreen` with Cancel/Sign
  (`views/tx_review/sign_view.py`, `gui/screens/tx_review/sign_screen.py`). Refuse to
  sign anything the device cannot render (`views/tx_review/base.py:69-90`).
- Seeds are BIP-39 only, 12/15/24 words (`models/seed.py:38-53`); a 25-word Monero
  mnemonic cannot be entered or scanned today. The mnemonic entry view itself is
  arity-agnostic (`seed_views.py:113-170`), so the work is: Monero word list plus
  checksum validation, a Seed variant that skips the BIP-39 checksum, a QR type for a
  25-word SeedQR, and the derivation memoized on the seed object like
  `Seed.cardano_root_key` (`seed.py:56-68`).

## Plan for the three screens

1. "Scan outputs -> key images QR": scan `xmr-output`, run `keyimages`, display
   `xmr-keyimage`.
2. "Scan unsigned tx -> summary -> confirm -> signed tx QR": scan `xmr-txunsigned`,
   run `show` to get inputs, fee, destinations and change (recomputed from the parsed
   set, never from host strings), confirm, run `sign`, display `xmr-txsigned`.
3. Seed load: 25-word entry reusing the keyboard view.
