# F-M10-032-1 — the CUDA distillation path is not run-to-run reproducible

**Found by:** Claude Opus 5 (implementer), 2026-08-26, while regenerating the bundles that
`F-M09-030-P1-1` invalidated. **Severity:** HIGH. **Status:** open.

## What was observed

The same command was run three times today against the same corpus, the same accepted vocabulary
generation, the same `INIT_SEED`, and the same device:

```
cargo run --release -p ti4-mlp --example distill -- \
    --corpus out/corpus/teacher-v2 --device cuda --out out/checkpoints/mlp
```

| run | epoch 1 train KL | epoch 1 validation KL | epoch 20 train KL | worst faction | top-1 agreement | gameplay abs delta |
|---|---|---|---|---|---|---|
| 13:33 | — | — | 0.00108 | xxcha 0.00162 | 93.868% | — |
| 18:19 | 0.11703 | 0.03099 | — | — | 93.743% | 0.0037 VP |
| 19:04 | 0.11704 | 0.03100 | 0.00109 | xxcha 0.00150 | 93.831% | 0.0157 VP |

Two facts matter more than the spread itself.

**The divergence is present at epoch 1**, so within the first 197 optimizer steps. It is therefore
not an accumulation over a long run that a tighter tolerance would absorb; the very first epoch of
two identically-configured runs does not agree.

**Selection-relevant quantities move.** Top-1 agreement spans 93.743%-93.868%, and the §6.1 gameplay
exit gate spans 0.0037-0.0157 VP across two runs — a factor of four.

## Why this is load-bearing rather than cosmetic

The immediate consequences are bounded: validation KL agrees to 0.00129 in every run, and the
gameplay gate's threshold is 0.1 VP, so every run passes with at least 6x margin. Nothing published
today is invalidated by this finding.

What it does invalidate is a class of *claim*. Specifically:

- **Bundle identity is not reproducible from its inputs.** A schema-6 manifest records the corpus,
  vocabulary generation, seed, compiler, and pinned runtime precisely so a bundle can be rebuilt and
  compared. On the CUDA path it cannot: rebuilding from the recorded inputs produces different
  weights. §4.4's verification story holds for *transport* (the checksums are exact) but not for
  *reconstruction*.
- **A single gameplay-gate measurement is not a gate.** The observed run-to-run spread is 0.012 VP
  on a 0.1 VP threshold. Today's margin is comfortable, so the gate's verdict is safe; but the
  spread has never been characterized, and a future measurement landing within ~0.012 of the
  threshold would carry no information. The gate needs a stated spread before it is read near its
  edge.
- **M09-030 pass 1 accepted M09-025's "deterministic CPU backend".** That acceptance is about the
  CPU path and is not disturbed. But no equivalent statement exists for the CUDA optimizer path, and
  this finding is the reason one is needed. That is M10-037's row.

## Most likely cause

Not diagnosed, and the finding does not depend on the diagnosis. The leading candidate is the
gradient of the fused `Tensor::embedding_bag` introduced for the CUDA path: its backward scatters
into the embedding table with atomics, so the summation order across concurrently-running blocks
varies between runs. Float addition is not associative, so identical inputs produce
bitwise-different gradients, and 3,940 optimizer steps amplify that into the spread above.

If that is right, the divergence is a property of the kernel and not of this code, and the remedies
are the usual ones: `torch.use_deterministic_algorithms`-equivalent settings where a deterministic
kernel exists, or an accepted statement that the CUDA optimizer path is reproducible only in
distribution.

## Recommended disposition

This is the implementer reporting a defect in the implementer's own work, so it needs an
independent reviewer. Suggested handling:

1. Do not claim CUDA reproducibility anywhere in M10-032/033 evidence; state the measured spread
   instead. The evidence for those rows is being rewritten against the regenerated bundles anyway.
2. Fold the CUDA determinism question into **M10-037**, which already owns the §7.1 CPU/CUDA gate,
   rather than opening a new row. Characterizing the gameplay gate's run-to-run spread belongs with
   it.
3. Leave the CPU path's determinism claims alone. Nothing here touches them.

## What would falsify this finding

A fourth CUDA run reproducing any earlier run's epoch-1 train KL to full printed precision. Three
runs disagreeing at the fifth significant figure in epoch 1 is the whole basis for the claim, and
one matching run would mean the cause is elsewhere -- in the corpus shard order or the shuffle --
and would need re-diagnosing rather than re-explaining.
