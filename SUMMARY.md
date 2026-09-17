# r/Monero opening (draft, under 200 words; final numbers after the 500-output run)

I built a Monero cold signer for a Raspberry Pi Zero 1.3 (single ARMv6 core, 512 MB,
no wireless) and ran it against an unmodified Feather 2.8.1 on stagenet. The device
speaks Feather's offline signing payloads directly: outputs export in, key images
out, unsigned tx set in, signed tx set out, the same bytes wallet2 uses, wrapped in
the Keystone xmr-* UR types. Feather imported the device-signed set and broadcast it;
it was mined in stagenet block 2209404.

Numbers on the Pi Zero (medians of 20 runs): decrypt a payload 0.74 s, key images
for 51 outputs 1.7 s, sign 2 inputs 4.5 s, sign 16 inputs 10 s, peak RSS 4 MB. A
51-output export is 20 KB, a 2-input unsigned tx 4 KB, its signed set 7 KB. The
500-output numbers are in the report.

It is Rust on monero-oxide, about 1500 lines, stateless, seed only in RAM. Code,
fixtures with provenance, and every raw measurement are in the repo. This is a
research spike, not a product; the camera round trip and the SeedSigner UI are the
next step.
