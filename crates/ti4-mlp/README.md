# ti4-mlp

The MLP policy and value networks of training on PyTorch/libtorch (`tch`): the model, PPO, the
checkpoints and the export of a bundle. The bots of the game do not use this crate: they play on
`ti4-infer` (`ti4_host::bots`, the `mlp:<name>` kinds that the server loads from `TI4_MODEL_DIR`).

## Native runtime

`ti4-mlp`, and every crate that links it (`cargo l-mlp`, `cargo t-mlp`, `cargo t-mlp-parity`), is
built and run with a matching CPU libtorch tree. `build.rs` checks the tree against its committed
manifest in `plans/artifacts/`.

On Linux or WSL, install the pinned Linux runtime once:

```bash
scripts/install_libtorch_cpu_linux.sh
export LIBTORCH="$PWD/out/libtorch-2.9.1-cpu-linux"
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH="$LIBTORCH/lib:${LD_LIBRARY_PATH:-}"
```

The Linux runtime is checked against
`plans/artifacts/libtorch-2.9.1-cpu-linux.manifest.json` during the build.

On Windows, restore the complete pinned CPU tree at
`out/libtorch-2.9.1-cpu` (the `LIBTORCH` of `.cargo/config.toml`) and make its `lib` directory
available on `PATH`:

```powershell
$env:LIBTORCH = "$PWD\out\libtorch-2.9.1-cpu"
$env:LIBTORCH_BYPASS_VERSION_CHECK = "1"
$env:PATH = "$env:LIBTORCH\lib;$env:PATH"
```
