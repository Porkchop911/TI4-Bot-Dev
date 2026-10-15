//! The MLP models of the server: every `.ti4m` of `TI4_MODEL_DIR` that its `index.json` lists,
//! loaded once at start into the registry of the process (`ti4_host::bots::register`). Each is the
//! in-process bot kind `mlp:<name>`. With no directory the server offers no MLP kind.
//!
//! `index.json` is the file `models/index.json` of the repository, as `scripts/pack-models.sh`
//! copies it next to the packed files: `{"models": [{"name", "label", "temperature", "source",
//! "sha256", "offered", "file"?}]}`. The server reads the name of the model (the name in the
//! file's header), `sha256`, `offered` and `file` (default `<name>.ti4m`); the label, the
//! temperature and the source are the pack script's (they are in the file's header too).
//!
//! `sha256` is the SHA-256 of the whole file, what `sha256sum` prints, and the address of the file
//! (`GET /api/models/<sha256>.ti4m`). It is not the file's trailer
//! ([`ti4_infer::Model::digest`], the digest of the bytes before the trailer), which the load
//! checks as well.
//!
//! Every model of the index is loaded, so that a game that seats it is recovered. Only an
//! `offered` one is a kind of a new table and is listed by `GET /api/models`; at most
//! [`MAX_OFFERED`] are.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use ti4_host::bots::MlpModel;

/// The directory of the models.
pub const MODEL_DIR_ENV: &str = "TI4_MODEL_DIR";
/// With `1`, the models run on the fastest backend of the CPU (`ti4_infer::Backend::fastest`,
/// fused multiply-add): its logits are not the bits of the page's, so a table of a seed is no
/// longer the same game in the page. A game of the server is still replayed from its decisions.
pub const FMA_ENV: &str = "TI4_INFER_FMA";
/// The most models a server offers: the row of an open seat in the lobby has room for three.
pub const MAX_OFFERED: usize = 3;

#[derive(Deserialize)]
struct Index {
    models: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    sha256: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    offered: bool,
}

/// A model of the directory, with its file.
#[derive(Debug, Clone)]
pub struct ServedModel {
    /// The model, as it is registered.
    pub model: MlpModel,
    /// The SHA-256 of the whole file, lowercase hex: its address.
    pub sha256: String,
    /// The size of the file.
    pub bytes: u64,
    /// The file.
    pub path: PathBuf,
    /// Whether new tables are offered it.
    pub offered: bool,
}

/// The models that the index loads without offering them. A model registered in another way (the
/// tests, the page) is offered.
static NOT_OFFERED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Whether a new table is offered the model of `name`: the setup options and the bot kinds of a
/// lobby list it.
#[must_use]
pub fn is_offered(name: &str) -> bool {
    !NOT_OFFERED.lock().expect("models lock").contains(name)
}

/// The backend the models are loaded on: `ti4_infer::Backend::fastest` with `TI4_INFER_FMA=1`,
/// else the fastest deterministic one.
#[must_use]
pub fn backend_from_env() -> ti4_infer::Backend {
    if std::env::var(FMA_ENV).is_ok_and(|value| value.trim() == "1") {
        ti4_infer::Backend::fastest()
    } else {
        ti4_infer::Backend::best()
    }
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn load_entry(
    dir: &Path,
    entry: &Entry,
    backend: ti4_infer::Backend,
) -> Result<ServedModel, String> {
    let file = entry
        .file
        .clone()
        .unwrap_or_else(|| format!("{}.ti4m", entry.name));
    if file.is_empty() || file.contains(['/', '\\']) || file.starts_with('.') {
        return Err(format!(
            "{}: the file {file:?} is not a plain file name",
            entry.name
        ));
    }
    if !is_sha256_hex(&entry.sha256) {
        return Err(format!(
            "{}: the sha256 {:?} is not 64 lowercase hex digits",
            entry.name, entry.sha256
        ));
    }
    let path = dir.join(&file);
    let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    if sha256 != entry.sha256 {
        return Err(format!(
            "{}: the SHA-256 is {sha256}, the index says {}",
            path.display(),
            entry.sha256
        ));
    }
    let model = MlpModel::from_bytes(&bytes, Some(backend))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if model.name() != entry.name {
        return Err(format!(
            "{}: the model is named {}, the index says {}",
            path.display(),
            model.name(),
            entry.name
        ));
    }
    Ok(ServedModel {
        model,
        sha256,
        bytes: bytes.len() as u64,
        path,
        offered: entry.offered,
    })
}

/// Load every model that `dir/index.json` lists, on `backend`.
///
/// # Errors
/// A message for an index that is not read, a name listed twice, more than [`MAX_OFFERED`]
/// offered models, or a model that is not loaded: its file is missing, its SHA-256 or its name is
/// not the one of the index, or it is not a model this build can play.
pub fn load_dir(dir: &Path, backend: ti4_infer::Backend) -> Result<Vec<ServedModel>, String> {
    let index_path = dir.join("index.json");
    let text = std::fs::read_to_string(&index_path)
        .map_err(|error| format!("{}: {error}", index_path.display()))?;
    let index: Index = serde_json::from_str(&text)
        .map_err(|error| format!("{}: {error}", index_path.display()))?;
    let mut names = BTreeSet::new();
    for entry in &index.models {
        if !names.insert(entry.name.as_str()) {
            return Err(format!(
                "{}: the model {} is listed twice",
                index_path.display(),
                entry.name
            ));
        }
    }
    let offered = index.models.iter().filter(|entry| entry.offered).count();
    if offered > MAX_OFFERED {
        return Err(format!(
            "{}: {offered} models are offered, at most {MAX_OFFERED} may be",
            index_path.display()
        ));
    }
    index
        .models
        .iter()
        .map(|entry| load_entry(dir, entry, backend))
        .collect()
}

/// Load the models of `TI4_MODEL_DIR` into the registry of the process and return them; none
/// without the variable.
///
/// # Errors
/// As [`load_dir`].
pub fn load_from_env() -> Result<Vec<ServedModel>, String> {
    let Some(dir) = std::env::var_os(MODEL_DIR_ENV) else {
        return Ok(Vec::new());
    };
    let models = load_dir(Path::new(&dir), backend_from_env())?;
    for served in &models {
        ti4_host::bots::register(served.model.clone());
        let mut not_offered = NOT_OFFERED.lock().expect("models lock");
        if served.offered {
            not_offered.remove(served.model.name());
        } else {
            not_offered.insert(served.model.name().to_owned());
        }
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_loads_the_models_of_its_index_and_checks_them() {
        let dir = std::env::temp_dir().join(format!("ti4_models_{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = ti4_host::bots::test_model_bytes();
        std::fs::write(dir.join("test.ti4m"), &bytes).unwrap();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        assert_ne!(
            sha,
            ti4_host::bots::test_model().digest(),
            "not the trailer"
        );
        let write = |entries: serde_json::Value| {
            let index = serde_json::json!({ "models": entries }).to_string();
            std::fs::write(dir.join("index.json"), index).unwrap();
        };
        let entry = |sha: &str, offered: bool| serde_json::json!({ "name": "test", "sha256": sha, "label": "x", "offered": offered });
        write(serde_json::json!([entry(&sha, true)]));
        let models = load_dir(&dir, ti4_infer::Backend::Scalar).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model.name(), "test");
        assert_eq!(models[0].sha256, sha);
        assert_eq!(models[0].bytes, bytes.len() as u64);
        assert!(models[0].offered);
        assert_eq!(
            models[0].model.model().backend(),
            ti4_infer::Backend::Scalar
        );

        write(serde_json::json!([entry(&"0".repeat(64), false)]));
        let error = load_dir(&dir, ti4_infer::Backend::Scalar).unwrap_err();
        assert!(error.contains("SHA-256"), "{error}");
        write(serde_json::json!([entry(&sha.to_uppercase(), false)]));
        assert!(load_dir(&dir, ti4_infer::Backend::Scalar).is_err());
        write(serde_json::json!([entry(&sha, false), entry(&sha, false)]));
        let error = load_dir(&dir, ti4_infer::Backend::Scalar).unwrap_err();
        assert!(error.contains("twice"), "{error}");
        let offered: Vec<_> = (0..4)
            .map(|n| serde_json::json!({ "name": format!("m{n}"), "sha256": sha, "offered": true }))
            .collect();
        write(serde_json::Value::Array(offered));
        let error = load_dir(&dir, ti4_infer::Backend::Scalar).unwrap_err();
        assert!(error.contains("at most 3"), "{error}");
        write(serde_json::json!([
            { "name": "test", "sha256": sha, "file": "../test.ti4m" }
        ]));
        assert!(load_dir(&dir, ti4_infer::Backend::Scalar).is_err());
        std::fs::remove_file(dir.join("index.json")).unwrap();
        assert!(load_dir(&dir, ti4_infer::Backend::Scalar).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
