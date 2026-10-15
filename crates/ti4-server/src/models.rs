//! The MLP models of the server: every `.ti4m` of `TI4_MODEL_DIR` that its `index.json` lists,
//! loaded once at start into the registry of the process (`ti4_host::bots::register`). Each is the
//! in-process bot kind `mlp:<name>`. With no directory the server offers no MLP kind.
//!
//! `index.json` is `{"models": [{"name": "...", "sha256": "...", "file": "..."}]}`: the name of
//! the model (the name in the file's header), the SHA-256 of the file (its trailer), and the file
//! in the directory (default `<name>.ti4m`). Other fields of an entry are not read here.

use std::path::Path;

use serde::Deserialize;
use ti4_host::bots::MlpModel;

/// The directory of the models.
pub const MODEL_DIR_ENV: &str = "TI4_MODEL_DIR";
/// With `1`, the models run on the fastest backend of the CPU (`ti4_infer::Backend::fastest`,
/// fused multiply-add): its logits are not the bits of the page's, so a table of a seed is no
/// longer the same game in the page. A game of the server is still replayed from its decisions.
pub const FMA_ENV: &str = "TI4_INFER_FMA";

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

/// Load every model that `dir/index.json` lists, on `backend`.
///
/// # Errors
/// A message for an index that is not read, or a model that is not: its file is missing, its
/// SHA-256 or its name is not the one of the index, or it is not a model this build can play.
pub fn load_dir(dir: &Path, backend: ti4_infer::Backend) -> Result<Vec<MlpModel>, String> {
    let index_path = dir.join("index.json");
    let text = std::fs::read_to_string(&index_path)
        .map_err(|error| format!("{}: {error}", index_path.display()))?;
    let index: Index = serde_json::from_str(&text)
        .map_err(|error| format!("{}: {error}", index_path.display()))?;
    index
        .models
        .iter()
        .map(|entry| {
            let file = entry
                .file
                .clone()
                .unwrap_or_else(|| format!("{}.ti4m", entry.name));
            let path = dir.join(&file);
            let bytes =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let model = MlpModel::from_bytes(&bytes, Some(backend))
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if !model.digest().eq_ignore_ascii_case(&entry.sha256) {
                return Err(format!(
                    "{}: the SHA-256 is {}, the index says {}",
                    path.display(),
                    model.digest(),
                    entry.sha256
                ));
            }
            if model.name() != entry.name {
                return Err(format!(
                    "{}: the model is named {}, the index says {}",
                    path.display(),
                    model.name(),
                    entry.name
                ));
            }
            Ok(model)
        })
        .collect()
}

/// Load the models of `TI4_MODEL_DIR` into the registry of the process and return them; none
/// without the variable.
///
/// # Errors
/// As [`load_dir`].
pub fn load_from_env() -> Result<Vec<MlpModel>, String> {
    let Some(dir) = std::env::var_os(MODEL_DIR_ENV) else {
        return Ok(Vec::new());
    };
    let models = load_dir(Path::new(&dir), backend_from_env())?;
    for model in &models {
        ti4_host::bots::register(model.clone());
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
        let sha = ti4_host::bots::test_model().digest().to_owned();
        let index = |sha: &str| {
            serde_json::json!({ "models": [{ "name": "test", "sha256": sha, "label": "x" }] })
                .to_string()
        };
        std::fs::write(dir.join("index.json"), index(&sha)).unwrap();
        let models = load_dir(&dir, ti4_infer::Backend::Scalar).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name(), "test");
        assert_eq!(models[0].model().backend(), ti4_infer::Backend::Scalar);

        std::fs::write(dir.join("index.json"), index(&"0".repeat(64))).unwrap();
        let error = load_dir(&dir, ti4_infer::Backend::Scalar).unwrap_err();
        assert!(error.contains("SHA-256"), "{error}");
        std::fs::remove_file(dir.join("index.json")).unwrap();
        assert!(load_dir(&dir, ti4_infer::Backend::Scalar).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
