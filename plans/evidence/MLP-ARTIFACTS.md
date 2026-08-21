# Artifact manifest

Checksums for files the plan's gates depend on. `out/` is gitignored. These hashes
identify the current machine-local inputs; M09-020 must create the bounded durable
fixtures and data-role manifest before corpus capture.

```
be792a2a207ced25d589162d875bae4fb1f320c8e5637045486db6a24ce5b55b *out/stage2_r6/final10000.json
aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5 *out/pools/full_np8_12_holdout.json
106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df *out/pools/full_np8_12_train.json
0d0fa9e5d7a3f9ce848ef2c52a4a4144183af7ca5c15082850874a18c039ca4a *out/stage1_hacanclone/frozen5000.json
```

Verified raw sizes: r6 checkpoint 33,886,908 bytes; Stage-1 checkpoint
6,261,762 bytes; validation pool 193,515 bytes; training pool 772,993 bytes.

Pools regenerate from committed code (`examples/generate_pool.rs`, seeds 1 and 777) --
both verified to reproduce bit-for-bit on commit 635d67d.
The seed-777 file's logical role is **validation**, not final, because its outcomes
have already informed the plan. M09-020 generates a new seed-20260822 pool, verifies
zero canonical board-hash overlap with both existing pools, records its checksum as
the sealed **final** role, and runs no policy against it.

The checkpoints do not regenerate. M09-020 deterministically compresses exactly
these two into `fixtures/mlp-baselines/` with raw/compressed hashes and a combined
compressed-size cap of 50 MiB. If that policy cannot be met, corpus capture remains
blocked pending explicit authority for a named durable external archive.
