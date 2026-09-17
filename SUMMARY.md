# r/Monero opening (under 200 words)

I built a Monero cold signer for a Raspberry Pi Zero 1.3 (single ARMv6 core, 512 MB,
no wireless) and ran it against an unmodified Feather 2.8.1 on stagenet. The device
speaks Feather's offline signing payloads directly: outputs export in, key images
out, unsigned tx set in, signed tx set out, the same bytes wallet2 uses, wrapped in
the Keystone xmr-* UR types. Feather imported the device-signed set and broadcast it;
it was mined in stagenet block 2209404.

Numbers on the Pi Zero, medians of 20 runs: decrypting a payload 0.75 s, key images
for 521 outputs 8.9 s, signing 2 inputs 4.5 s, signing 16 inputs 9.0 s, peak RSS
4 MB. Payload sizes: a 521-output export is 57 KB (478 QR frames at 120 bytes each),
its key image reply 50 KB, a 2-input unsigned tx 4.2 KB, its signed set 6.7 KB. The
big export is a one-time cost; Feather's default export only carries outputs whose
key images it does not know yet.

It is Rust on monero-oxide, about 1500 lines, stateless, seed only in RAM. Code,
fixtures with provenance, and every raw measurement are in the repo. This is a
research spike, not a product; the camera round trip and the SeedSigner UI are the
next step.
