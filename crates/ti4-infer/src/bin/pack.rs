//! `ti4-pack <bundle dir> <name> <out.ti4m> [--label <text>] [--temperature <t>]`
//!
//! Reads and checks a training bundle, packs it into one `.ti4m` file, reads the file back, and
//! prints its name, size, the SHA-256 of the whole file (what `sha256sum` prints, the `sha256` of
//! `models/index.json` and the address of the file on a server) and its trailer. The label defaults to the name and the temperature
//! to 0.25. The file is written beside its destination and renamed into place.

use std::path::PathBuf;
use std::process::ExitCode;

use sha2::Digest;
use ti4_infer::format::{self, DEFAULT_TEMPERATURE};
use ti4_infer::{Meta, bundle};

const USAGE: &str =
    "usage: ti4-pack <bundle dir> <name> <out.ti4m> [--label <text>] [--temperature <t>]";

fn run() -> Result<String, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut label: Option<String> = None;
    let mut temperature = DEFAULT_TEMPERATURE;
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
    let bytes = format::write(&meta, &parts).map_err(|error| error.to_string())?;
    let model =
        format::read(&bytes).map_err(|error| format!("the packed file does not read: {error}"))?;

    let out = PathBuf::from(out);
    let staging = out.with_extension("ti4m.tmp");
    std::fs::write(&staging, &bytes)
        .map_err(|error| format!("writing {}: {error}", staging.display()))?;
    std::fs::rename(&staging, &out)
        .map_err(|error| format!("renaming to {}: {error}", out.display()))?;
    Ok(format!(
        "{} \"{}\" width {} blocks {} schema {} -> {} ({} bytes, sha256 {:x}, trailer {})",
        model.name(),
        model.label(),
        model.width(),
        model.residual_blocks(),
        model.source().schema,
        out.display(),
        bytes.len(),
        sha2::Sha256::digest(&bytes),
        model.digest(),
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
