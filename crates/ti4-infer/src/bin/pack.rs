//! `ti4-pack <bundle dir> <name> <out.ti4m> [--label <text>] [--temperature <t>] [--quant int8]`
//!
//! Reads and checks a training bundle, packs it into one `.ti4m` file, reads the file back, and
//! prints its name, size, the SHA-256 of the whole file (what `sha256sum` prints, the `sha256` of
//! `models/index.json` and the address of the file on a server) and its trailer. The label defaults to the name and the temperature
//! to 0.25. The file is written beside its destination and renamed into place.
//!
//! `--quant int8` writes the int8 file of the rollouts of training (section H of
//! `web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`): it loads on `Avx2Vnni` only, so the tool reads
//! it back only on a CPU with AVX-VNNI (elsewhere it checks the header and the trailer), and no
//! server offers it.

use std::path::PathBuf;
use std::process::ExitCode;

use sha2::Digest;
use ti4_infer::format::{self, DEFAULT_TEMPERATURE};
use ti4_infer::{Backend, InferError, Meta, Quant, bundle};

const USAGE: &str = "usage: ti4-pack <bundle dir> <name> <out.ti4m> [--label <text>] \
                     [--temperature <t>] [--quant int8]";

fn run() -> Result<String, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut label: Option<String> = None;
    let mut temperature = DEFAULT_TEMPERATURE;
    let mut quant = Quant::F32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--label" => label = Some(args.next().ok_or(USAGE)?),
            "--temperature" => {
                let text = args.next().ok_or(USAGE)?;
                temperature = text
                    .parse()
                    .map_err(|_| format!("temperature {text} is not a number"))?;
            }
            "--quant" => {
                quant = match args.next().ok_or(USAGE)?.as_str() {
                    "int8" => Quant::Int8,
                    "f32" => Quant::F32,
                    other => return Err(format!("--quant {other}: expected int8 or f32")),
                };
            }
            "-h" | "--help" => return Err(USAGE.to_owned()),
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ => positional.push(arg),
        }
    }
    let [bundle_dir, name, out] = <[String; 3]>::try_from(positional).map_err(|_| USAGE)?;
    let meta = Meta {
        label: label.unwrap_or_else(|| name.clone()),
        name,
        temperature,
    };
    let parts = bundle::read(&PathBuf::from(&bundle_dir)).map_err(|error| error.to_string())?;
    let bytes = format::write_as(&meta, &parts, quant).map_err(|error| error.to_string())?;
    // An int8 file loads only where the CPU has AVX-VNNI; elsewhere `read` checks everything up
    // to the backend, and refuses only that.
    let model = match format::read(&bytes) {
        Ok(model) => Some(model),
        Err(InferError::Backend(Backend::Avx2Vnni)) if quant == Quant::Int8 => None,
        Err(error) => return Err(format!("the packed file does not read: {error}")),
    };

    let out = PathBuf::from(out);
    let staging = out.with_extension("ti4m.tmp");
    std::fs::write(&staging, &bytes)
        .map_err(|error| format!("writing {}: {error}", staging.display()))?;
    std::fs::rename(&staging, &out)
        .map_err(|error| format!("renaming to {}: {error}", out.display()))?;
    let trailer = model.as_ref().map_or_else(
        || "not read back: this CPU has no avx2-vnni".to_owned(),
        |model| model.digest().to_owned(),
    );
    Ok(format!(
        "{} \"{}\" width {} blocks {} schema {} {} -> {} ({} bytes, sha256 {:x}, trailer {})",
        meta.name,
        meta.label,
        parts.width,
        parts.blocks.len(),
        parts.source.schema,
        quant.name(),
        out.display(),
        bytes.len(),
        sha2::Sha256::digest(&bytes),
        trailer,
    ))
}

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ti4-pack: {error}");
            ExitCode::FAILURE
        }
    }
}
