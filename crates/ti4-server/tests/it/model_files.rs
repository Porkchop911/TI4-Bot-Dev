//! `GET /api/models` and `GET /api/models/<sha256>.ti4m` on the public server: the offered models
//! of `models/index.json`, packed here in memory from their committed bundles (the test does not
//! read `out/`), with the SHA-256 of the whole file as the address and an immutable cache header.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

use ti4_server::create_app;
use ti4_server::public::PublicConfig;
use ti4_server::public::ratelimit::RateLimits;
use ti4_server::session::GameRegistry;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The entries of `models/index.json`, each packed from its bundle in the repository, as
/// `scripts/pack-models.sh` packs it, into `dir`. The shas of the index must be those of the files.
fn pack_committed_models(dir: &Path) -> Vec<Value> {
    let index: Value =
        serde_json::from_str(&std::fs::read_to_string(repo().join("models/index.json")).unwrap())
            .unwrap();
    let entries = index["models"].as_array().unwrap().clone();
    assert!(!entries.is_empty(), "models/index.json lists a model");
    for entry in &entries {
        let name = entry["name"].as_str().unwrap();
        let bundle = repo().join(entry["source"]["bundle"].as_str().unwrap());
        assert_eq!(
            sha256(&std::fs::read(bundle.join("manifest.json")).unwrap()),
            entry["source"]["manifest_sha256"].as_str().unwrap(),
            "{name}: the sha of the manifest"
        );
        let meta = ti4_infer::Meta {
            name: name.to_owned(),
            label: entry["label"].as_str().unwrap().to_owned(),
            temperature: entry["temperature"].as_f64().unwrap() as f32,
        };
        let parts = ti4_infer::bundle::read(&bundle).unwrap();
        let bytes = ti4_infer::format::write(&meta, &parts).unwrap();
        assert_eq!(
            sha256(&bytes),
            entry["sha256"].as_str().unwrap(),
            "{name}: the sha of the packed file (scripts/pack-models.sh --update-index)"
        );
        std::fs::write(dir.join(format!("{name}.ti4m")), bytes).unwrap();
    }
    entries
}

async fn serve_public(registry: GameRegistry) -> String {
    let config = PublicConfig {
        public: true,
        rate_limits: Some(RateLimits::default()),
        ..PublicConfig::default()
    };
    let app = create_app(Arc::new(registry.with_public(config)));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn the_server_lists_the_offered_models_and_serves_their_files_by_sha() {
    let dir = std::env::temp_dir().join(format!("ti4_model_files_{:016x}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut entries = pack_committed_models(&dir);
    // A model that is loaded and not offered: neither listed nor served.
    let hidden = ti4_host::bots::test_model_bytes();
    std::fs::write(dir.join("test.ti4m"), &hidden).unwrap();
    entries.push(json!({ "name": "test", "sha256": sha256(&hidden), "offered": false }));
    let offered: Vec<&Value> = entries.iter().filter(|e| e["offered"] == true).collect();
    assert!(!offered.is_empty() && offered.len() <= 3);
    // The .br of the first offered model, as the pack script writes it when it is smaller.
    let first = offered[0]["name"].as_str().unwrap().to_owned();
    std::fs::write(dir.join(format!("{first}.ti4m.br")), b"brotli of the file").unwrap();
    std::fs::write(
        dir.join("index.json"),
        json!({ "models": entries }).to_string(),
    )
    .unwrap();

    let models = ti4_server::models::load_dir(&dir, ti4_infer::Backend::Scalar).unwrap();
    let base = serve_public(GameRegistry::new().with_models(models)).await;
    let client = reqwest::Client::new();

    let list: Value = client
        .get(format!("{base}/api/models"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let listed = list.as_array().unwrap();
    assert_eq!(listed.len(), offered.len(), "{list}");
    for (listing, entry) in listed.iter().zip(&offered) {
        let name = entry["name"].as_str().unwrap();
        let file = std::fs::read(dir.join(format!("{name}.ti4m"))).unwrap();
        assert_eq!(listing["id"], entry["name"]);
        assert_eq!(listing["label"], entry["label"]);
        assert_eq!(listing["sha256"], entry["sha256"]);
        assert_eq!(listing["bytes"], file.len());
    }

    let sha = offered[0]["sha256"].as_str().unwrap();
    let url = format!("{base}/api/models/{sha}.ti4m");
    let plain = client.get(&url).send().await.unwrap();
    assert_eq!(plain.status(), StatusCode::OK);
    let headers = plain.headers().clone();
    assert_eq!(
        headers["cache-control"],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert!(headers.get("content-encoding").is_none());
    assert_eq!(sha256(&plain.bytes().await.unwrap()), sha);

    let br = client
        .get(&url)
        .header("accept-encoding", "gzip, deflate, br")
        .send()
        .await
        .unwrap();
    assert_eq!(br.status(), StatusCode::OK);
    assert_eq!(br.headers()["content-encoding"], "br");
    assert_eq!(
        br.headers()["cache-control"],
        "public, max-age=31536000, immutable"
    );
    assert!(
        br.headers()
            .get_all("vary")
            .iter()
            .any(|v| v.to_str().unwrap().contains("ccept-Encoding")),
        "{:?}",
        br.headers()
    );
    assert_eq!(&br.bytes().await.unwrap()[..], b"brotli of the file");

    for missing in [
        format!("{base}/api/models/{}.ti4m", sha256(&hidden)),
        format!("{base}/api/models/{}.ti4m", "0".repeat(64)),
        format!("{base}/api/models/{sha}"),
    ] {
        let response = client.get(&missing).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{missing}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
