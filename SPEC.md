# Monero SeedSigner research spike

Objective: produce verifiable numbers and a demo video showing that a stateless, air-gapped
SeedSigner-class device (Raspberry Pi Zero 1.3, ARMv6, 512 MB, no wireless silicon) can act
as a Monero cold signer against an unmodified, released Feather Wallet, so the results can be
posted to the Monero community (r/Monero, #monero-community Matrix) as the basis of a CCS
proposal. This is a research spike, not the product. Nothing here needs a UI beyond what is
required for the video.

Owner: Angel Castillo (AngelCastilloB). This document is written for a fresh agent session
with no prior context. Everything needed is in this file or in the linked repos.

## 1. Background you need

- The device: a fork of SeedSigner (Python app on a minimal Linux booting from RAM) that
  already ships for Cardano: `~/Sources/angel/cardano-seedsigner` (device app) and
  `~/Sources/angel/cardano-seedsigner-os` (image build). It already has animated QR (UR)
  scanning via the Pi camera, UR display on the 240x240 LCD, seed loading (SeedQR, dice,
  words), and a menu shell. Reuse all of it. Do not build a wallet.
- The pattern for a signing core: `~/Sources/angel/libzcash-signer`, a Rust workspace that
  parses a PCZT, derives keys, signs on device, and ships a `spike-bench` binary with
  per-phase timing and peak RSS, cross-compiled statically for
  `arm-unknown-linux-musleabihf`. See its `PI_RUN.md` for how numbers were produced on the
  real Pi. Copy this structure for Monero.
- The hot wallet is Feather (https://github.com/feather-wallet/feather). Its offline
  transaction signing wizard (`src/wizard/offline_tx_signing/`) exchanges data with a cold
  signer as animated QR using exactly four UR types from Keystone's open registry:
  `xmr-output`, `xmr-keyimage`, `xmr-txunsigned`, `xmr-txsigned`. Type definitions:
  https://github.com/KeystoneHQ/keystone-sdk-rust/tree/master/libs/ur-registry/src/monero
  (files `xmr_output.rs`, `xmr_keyimage.rs`, `xmr_txunsigned.rs`, `xmr_txsigned.rs`).
  Feather's docs only list Ledger and Trezor as "hardware wallets"; the UR flow lives under
  offline signing and is what Keystone uses. Confirm the exact CBOR layout from the registry
  source, not from docs.
- Cake Wallet ships Cupcake (https://github.com/cake-tech/cupcake, GPL-3, Flutter), an app
  that turns an old phone into a QR air-gap signer for Monero. Cake lists Keystone as
  "coming soon" for Monero. Cupcake very likely speaks the same four UR types; verify by
  reading its `lib/utils/urqr*.dart` and dependencies. Cake compatibility is a bonus target,
  Feather is the primary.
- Monero today uses CLSAG ring signatures. Cold signing today is a four step round trip:
  hot wallet exports outputs -> cold computes key images and exports them -> hot builds an
  unsigned tx (with ring member data) -> cold signs -> hot broadcasts. Payloads can be large.
- FCMP++ (with the Carrot address scheme) is Monero's next hard fork. It is committed but
  has no date: as of September 2026 audits are at request-for-quote stage, beta stressnet v3
  has not launched, and the hot/cold wallet flow for it is being written in
  https://github.com/seraphis-migration/monero/pull/52 (tracked in
  https://github.com/seraphis-migration/monero/issues/53). Under FCMP++ the device only
  produces the spend-authorization-and-linkability proof and the host does the membership
  proof, and Carrot lets a view-balance hot wallet compute key images, so the round trip
  shrinks. This spike targets TODAY's protocol. FCMP++ is version two and out of scope,
  except for one paragraph in the report (section 6).
- Rust implementation of Monero wallet logic: monero-oxide
  (https://github.com/monero-oxide/monero-oxide, formerly monero-serai; the `fcmp++` branch
  is the future work, use the default branch). Check whether its wallet crates can produce
  key images from exported outputs and sign an unsigned transaction in the format Feather
  exports. If they cannot, the fallback is the C++ `wallet2` cold-signing code from
  monero-project/monero (`sign_tx`, `export_outputs`, `import_key_images`) cross-compiled
  for ARMv6. Decide this on day one; it is the largest risk.

## 2. Deliverables

1. `xmr-signer` Rust workspace in this directory (or a sibling repo `Biglup/libmonero-signer`),
   structured like libzcash-signer:
   - `libmonero-signer`: parse `xmr-output` payload -> compute key images -> emit
     `xmr-keyimage`; parse `xmr-txunsigned` -> sign -> emit `xmr-txsigned`. Seed in,
     everything derived in memory, nothing written to disk.
   - `vector-gen`: produces committed test fixtures from a stagenet wallet (outputs export,
     unsigned tx) with documented provenance.
   - `spike-bench`: runs the full pipeline against fixtures and prints per-phase timing and
     peak RSS. Static ARMv6 build. `PI_RUN.md` explaining how to reproduce on the device.
2. `REPORT.md` with the measurements in section 5, in the table layout given there.
3. A demo video: Feather (released build, stagenet) on a laptop, the device across from it,
   full round trip ending in Feather broadcasting the signed transaction. Under two minutes.
   Stage 2 of the plan; stage 1 numbers do not depend on it.
4. One paragraph on FCMP++ implications, written from the hot/cold PR, for the report.

## 3. Plan

Stage 1, numbers without a camera (target: one week)

1. Day one gate: cross-compile monero-oxide's wallet crates to
   `arm-unknown-linux-musleabihf` and run a trivial key image computation on the Pi. If it
   builds and runs, continue in Rust. If it does not within a day, switch to the wallet2 C++
   path or to a Pi Zero 2 W target (aarch64), and record the decision in REPORT.md.
2. Build a stagenet wallet in Feather with three histories: about 50, 200 and 500 owned
   outputs (use a stagenet faucet and self-sends; document how). For each, run Feather's
   offline signing wizard to produce the outputs export and an unsigned transaction with 2
   inputs and one with 16 inputs. Save the raw UR payloads as fixtures (Feather can show the
   UR; capture it, or use the wizard's file export if present, and note which).
3. Implement `libmonero-signer` against those fixtures: decode UR/CBOR per the Keystone
   registry, compute key images, sign. Validate the signed tx by importing it back into
   Feather (stagenet) and broadcasting.
4. Measure everything in section 5 with `spike-bench` on the real Pi Zero 1.3. Also compute
   animated QR frame counts for each payload using the same fountain encoder parameters the
   SeedSigner shell uses (check `cardano-seedsigner` for the frame size it displays).

Stage 2, camera round trip and video (target: one week)

5. Add the Monero flow to the SeedSigner fork behind the existing FFI pattern: three screens,
   "scan outputs -> show key images QR", "scan unsigned tx -> show what it does -> confirm ->
   show signed tx QR", plus seed load reusing the existing screens. Show on the confirm screen
   at least: number of inputs, total out, fee, destination address (truncated), change.
   Recompute these from the parsed transaction, never from host-supplied strings.
6. Measure scan time per direction on the Pi camera for the 50/200/500 output wallets and
   the boot-to-signed wall clock. Add to the report.
7. Record the video on stagenet. Then, if time allows, repeat the round trip against Cake with
   the Cupcake flow and note whether it works unchanged.

## 4. Constraints

- Device is stateless: no seed or key material persisted, ever. Seed only in RAM.
- No network on the device. Fixtures move by microSD or by QR only.
- Stagenet only. No mainnet funds anywhere in this spike.
- Do not modify Feather. If a Feather change is needed to make the flow work, that is a
  finding, not a task: write it down in the report with the smallest possible diff.
- Reuse the SeedSigner shell's existing UR scan/display code. Do not write a new transport.
- Keep the repo public from the first commit and commit fixtures with provenance notes.
- ASCII only in code and docs. No em dashes, curly quotes, or unicode arrows.

## 5. Measurements to report

Fill this table exactly, numbers from the real Pi Zero 1.3 unless a row says otherwise.
State the XMR wallet size for each row.

| Wallet outputs | Outputs export bytes | QR frames | Key image export bytes | QR frames | Unsigned tx bytes (2 in) | QR frames | Signed tx bytes | QR frames |
|---|---|---|---|---|---|---|---|---|
| ~50 | | | | | | | | |
| ~200 | | | | | | | | |
| ~500 | | | | | | | | |

| Operation (Pi Zero 1.3) | Median ms (20 runs) | Peak RSS MB |
|---|---|---|
| Parse outputs export (500 outputs) | | |
| Compute key images (500 outputs) | | |
| Parse unsigned tx (2 inputs) | | |
| Sign (2 inputs) | | |
| Sign (16 inputs) | | |

| Camera round trip (stage 2) | Seconds |
|---|---|
| Scan outputs export (500 outputs) | |
| Scan unsigned tx (16 inputs) | |
| Feather scans key images | |
| Feather scans signed tx | |
| Power on to signed tx (2 inputs, SeedQR + passphrase) | |

Also record: monero-oxide commit, Feather version, Pi kernel/image, toolchain versions, and
whether Cupcake/Cake worked unchanged (yes/no/not tested).

## 6. Report structure (REPORT.md)

1. One paragraph: what was built, what it proves, what it does not.
2. The three tables above.
3. Findings: anything that did not work, any Feather-side change needed, payload/UX notes
   (is the 500-output export pleasant or painful through a camera).
4. Decision record: Rust vs C++ path, Pi Zero 1.3 vs 2 W, with the reason.
5. FCMP++ paragraph: from seraphis-migration PR #52, what the device would compute under
   FCMP++ and Carrot and how the round trip changes. One paragraph, no speculation beyond the
   PR.
6. Link to the video and to the repo.

## 7. Definition of done

- REPORT.md complete with real numbers, no placeholders.
- Video published (unlisted is fine) showing Feather broadcasting a device-signed stagenet tx.
- `spike-bench` reproducible by a third party following PI_RUN.md.
- A short summary (under 200 words) suitable as the opening of a r/Monero post, written in
  plain first person, no marketing language, numbers first.

## 8. Out of scope

FCMP++ or Carrot implementation, Sapling-style anything, mainnet, multisig, a device UI beyond
the three screens, Cake/Cupcake beyond a compatibility check, any CCS proposal text.
