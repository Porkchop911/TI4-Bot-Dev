//! The model file and the forward pass of `ti4-infer`, through the public API.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;
use ti4_infer::format::{self, VERSION};
use ti4_infer::synthetic::Rng;
use ti4_infer::{
    EMBED_DIM, InferError, LEGACY_HEADS, Meta, Model, Parts, Scratch, SparseOption, bundle,
    total_order_key,
};

fn meta() -> Meta {
    Meta {
        name: "synthetic".to_owned(),
        label: "Synthetic r2".to_owned(),
        temperature: 0.25,
    }
}

/// A model with seeded random weights, of the legacy residual schema 8.
fn synthetic(seed: u64, width: usize, capacity: usize, blocks: usize) -> Parts {
    ti4_infer::synthetic::parts(seed, width, capacity, blocks)
}

/// Options with unsorted and repeated columns, an empty one and an already sorted one.
fn options(rng: &mut Rng, count: usize, capacity: usize) -> Vec<SparseOption> {
    ti4_infer::synthetic::options(rng.next_u64(), count, capacity, 40)
}

fn relu(x: f32) -> f32 {
    if x > 0.0 { x } else { 0.0 }
}

/// The forward pass in plain loops over the unpacked weights, in the order `ti4_infer::model`
/// fixes for every sum.
fn reference(parts: &Parts, faction: usize, head: usize, options: &[SparseOption]) -> Vec<f32> {
    let w = parts.width;
    let heads = parts.heads.len();
    let dot = |x: &[f32], matrix: &[f32], out: usize| {
        let mut sum = 0.0f32;
        for k in 0..w {
            sum += x[k] * matrix[out * w + k];
        }
        sum
    };
    options
        .iter()
        .map(|option| {
            let mut pairs: Vec<(i64, f32)> = option
                .columns
                .iter()
                .copied()
                .zip(option.values.iter().copied())
                .collect();
            pairs.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then(total_order_key(a.1).cmp(&total_order_key(b.1)))
            });
            let mut folded: Vec<(usize, f32)> = Vec::new();
            for (column, value) in pairs {
                let column = usize::try_from(column).unwrap();
                match folded.last_mut() {
                    Some(last) if last.0 == column => last.1 += value,
                    _ => folded.push((column, value)),
                }
            }
            let mut z = vec![0.0f32; w];
            for i in 0..w {
                let mut sum = 0.0f32;
                for &(column, value) in &folded {
                    sum += value * parts.w1[column * w + i];
                }
                let identity = if i < EMBED_DIM {
                    parts.embedding[faction * EMBED_DIM + i]
                } else {
                    0.0
                };
                z[i] = relu((sum + identity) + parts.b1[i]);
            }
            let mut z: Vec<f32> = (0..w)
                .map(|i| relu(dot(&z, &parts.w2, i) + parts.b2[i]))
                .collect();
            for block in &parts.blocks {
                let hidden: Vec<f32> = (0..w)
                    .map(|i| relu(dot(&z, &block.w_in, i) + block.b_in[i]))
                    .collect();
                z = (0..w)
                    .map(|i| (z[i] + dot(&hidden, &block.w_out, i)) + block.b_out[i])
                    .collect();
            }
            let at = faction * heads + head;
            let mut lanes = [0.0f32; 4];
            for i in 0..w {
                let weight = parts.w_shared[head * w + i] + parts.delta[at * w + i];
                lanes[i % 4] += z[i] * weight;
            }
            let bias = parts.b_shared[head] + parts.b_delta[at];
            ((lanes[0] + lanes[1]) + (lanes[2] + lanes[3])) + bias
        })
        .collect()
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

/// A file with its header rewritten by `edit`, and a valid trailer.
fn rewrite(bytes: &[u8], edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let (header, data) = format::split(bytes).expect("a model file");
    let mut header: Value = serde_json::from_slice(header).expect("JSON");
    edit(&mut header);
    format::assemble(&serde_json::to_vec(&header).unwrap(), data)
}

#[test]
fn a_model_file_round_trips() {
    let parts = synthetic(7, 128, 300, 2);
    let bytes = format::write(&meta(), &parts).expect("packs");
    assert_eq!(bytes[..4], *b"TI4M");
    assert_eq!(
        format::write(&meta(), &parts).unwrap(),
        bytes,
        "the same bytes every time"
    );

    let model = Model::from_bytes(&bytes).expect("reads");
    assert_eq!(model.name(), "synthetic");
    assert_eq!(model.label(), "Synthetic r2");
    assert_eq!(model.temperature(), 0.25);
    assert_eq!(model.width(), 128);
    assert_eq!(model.capacity(), 300);
    assert_eq!(model.residual_blocks(), 2);
    assert_eq!(model.heads(), parts.heads.as_slice());
    assert_eq!(model.head_index("combat"), Some(12));
    assert_eq!(model.faction_index("sol"), Some(27));
    assert_eq!(model.source(), &parts.source);
    assert_eq!(model.slots_json(), parts.slots);
    assert_eq!(model.battle_json(), None);
    {
        use sha2::Digest as _;
        let body = &bytes[..bytes.len() - 32];
        assert_eq!(model.digest(), format!("{:x}", sha2::Sha256::digest(body)));
    }

    // The sections are where the header says, 64-byte aligned.
    let (header, data) = format::split(&bytes).unwrap();
    let header: Value = serde_json::from_slice(header).unwrap();
    assert_eq!(header["data_bytes"].as_u64().unwrap() as usize, data.len());
    for section in header["sections"].as_array().unwrap() {
        assert_eq!(section["offset"].as_u64().unwrap() % 64, 0, "{section}");
    }
    assert_eq!((bytes.len() - 32 - data.len()) % 64, 0);

    // A battle predictor travels with schemas 11 and 12.
    let mut battle = synthetic(8, 128, 64, 1);
    battle.heads = ti4_infer::DIPLOMACY_HEADS
        .iter()
        .map(|head| (*head).to_owned())
        .collect();
    battle.w_shared = vec![0.0; 15 * 128];
    battle.b_shared = vec![0.0; 15];
    battle.delta = vec![0.0; 33 * 15 * 128];
    battle.b_delta = vec![0.0; 33 * 15];
    battle.source.schema = 12;
    battle.source.projection_abi = 4;
    battle.battle = Some(r#"{"predictor":1}"#.to_owned());
    let model = Model::from_bytes(&format::write(&meta(), &battle).unwrap()).unwrap();
    assert_eq!(model.battle_json(), Some(r#"{"predictor":1}"#));
    assert_eq!(model.head_index("diplomacy"), Some(14));
}

#[test]
fn the_forward_is_the_plain_loops_bit_for_bit() {
    let parts = synthetic(11, 128, 500, 2);
    let model = Model::from_bytes(&format::write(&meta(), &parts).unwrap()).unwrap();
    let mut rng = Rng::new(3);
    let mut scratch = Scratch::new();
    let mut out = Vec::new();
    // 1 to 9 options: every tail of the four-row micro-kernel, and more than one block of rows.
    for count in 1..=9 {
        let options = options(&mut rng, count, parts.capacity);
        for (faction, head) in [(0, 0), (7, 4), (32, 13), (27, 12)] {
            let expected = reference(&parts, faction, head, &options);
            model
                .forward_with(&mut scratch, faction, head, &options, &mut out)
                .expect("forward");
            assert_eq!(
                bits(&out),
                bits(&expected),
                "{count} options, faction {faction}, head {head}"
            );
            assert_eq!(model.forward(faction, head, &options).unwrap(), out);
        }
    }
    // Not every logit is the same: the trunk passes something through.
    let logits = model
        .forward(0, 0, &options(&mut rng, 12, parts.capacity))
        .unwrap();
    assert!(logits.iter().any(|logit| *logit != logits[0]), "{logits:?}");
}

#[test]
fn a_forward_refuses_what_the_gather_refuses() {
    let parts = synthetic(5, 128, 50, 0);
    let model = Model::from_bytes(&format::write(&meta(), &parts).unwrap()).unwrap();
    let one = |columns: Vec<i64>, values: Vec<f32>| vec![SparseOption { columns, values }];
    assert!(matches!(
        model.forward(0, 0, &[]),
        Err(InferError::EmptyLegalSet)
    ));
    assert!(matches!(
        model.forward(33, 0, &one(vec![1], vec![1.0])),
        Err(InferError::Faction(33))
    ));
    assert!(matches!(
        model.forward(0, 14, &one(vec![1], vec![1.0])),
        Err(InferError::Head(14))
    ));
    assert!(matches!(
        model.forward(0, 0, &one(vec![50], vec![1.0])),
        Err(InferError::OutOfRange {
            column: 50,
            capacity: 50
        })
    ));
    assert!(matches!(
        model.forward(0, 0, &one(vec![1], vec![f32::INFINITY])),
        Err(InferError::NotFinite(_))
    ));
    assert!(matches!(
        model.forward(0, 0, &one(vec![1, 2], vec![1.0])),
        Err(InferError::Ragged {
            columns: 2,
            values: 1
        })
    ));
}

#[test]
fn a_model_file_with_a_bad_sha_is_refused() {
    let bytes = format::write(&meta(), &synthetic(1, 128, 40, 1)).unwrap();
    let mut corrupt = bytes.clone();
    let at = corrupt.len() - 100;
    corrupt[at] ^= 1;
    assert!(matches!(
        Model::from_bytes(&corrupt),
        Err(InferError::Digest { .. })
    ));
    let mut trailer = bytes;
    let last = trailer.len() - 1;
    trailer[last] ^= 0x80;
    assert!(matches!(
        Model::from_bytes(&trailer),
        Err(InferError::Digest { .. })
    ));
}

#[test]
fn a_model_file_of_another_version_or_schema_is_refused() {
    let bytes = format::write(&meta(), &synthetic(1, 128, 40, 1)).unwrap();
    let mut version = bytes.clone();
    version[4..8].copy_from_slice(&(VERSION + 1).to_le_bytes());
    assert!(matches!(Model::from_bytes(&version), Err(InferError::Version(v)) if v == VERSION + 1));
    let mut magic = bytes.clone();
    magic[0] = b'X';
    assert!(matches!(Model::from_bytes(&magic), Err(InferError::Magic)));
    for schema in [6, 13] {
        let other = rewrite(&bytes, |header| header["source"]["schema"] = schema.into());
        let error = Model::from_bytes(&other).unwrap_err();
        assert!(matches!(error, InferError::Format(_)), "{error}");
    }
    let extra = rewrite(&bytes, |header| header["extra"] = 1.into());
    assert!(matches!(
        Model::from_bytes(&extra),
        Err(InferError::Format(_))
    ));
    // The rewrite itself keeps a valid file.
    Model::from_bytes(&rewrite(&bytes, |_| {})).expect("an unchanged header reads");
}

#[test]
fn a_width_that_is_not_a_multiple_of_8_is_refused() {
    let mut parts = synthetic(2, 128, 40, 0);
    let bytes = format::write(&meta(), &parts).unwrap();
    for width in [12, 130, 0, 8] {
        let other = rewrite(&bytes, |header| header["width"] = width.into());
        assert!(
            matches!(Model::from_bytes(&other), Err(InferError::Width(w)) if w == width),
            "width {width}"
        );
    }
    parts.width = 132;
    assert!(matches!(
        format::write(&meta(), &parts),
        Err(InferError::Width(132))
    ));
}

#[test]
fn a_truncated_model_file_is_refused() {
    let bytes = format::write(&meta(), &synthetic(3, 128, 40, 1)).unwrap();
    for keep in [
        bytes.len() - 1,
        bytes.len() - 32,
        bytes.len() / 2,
        200,
        40,
        11,
        0,
    ] {
        let error = Model::from_bytes(&bytes[..keep]).unwrap_err();
        assert!(
            matches!(error, InferError::Truncated { .. } | InferError::Magic),
            "{keep} bytes: {error}"
        );
    }
    let mut longer = bytes;
    longer.push(0);
    assert!(matches!(
        Model::from_bytes(&longer),
        Err(InferError::Truncated { .. })
    ));
}

#[test]
fn the_writer_refuses_parts_it_cannot_pack() {
    let parts = synthetic(4, 128, 40, 1);
    let mut short = parts.clone();
    short.b2.pop();
    assert!(matches!(
        format::write(&meta(), &short),
        Err(InferError::Parts(_))
    ));
    let mut nan = parts.clone();
    nan.blocks[0].w_out[5] = f32::NAN;
    assert!(matches!(
        format::write(&meta(), &nan),
        Err(InferError::Parts(_))
    ));
    let mut name = meta();
    name.name = "Has Space".to_owned();
    assert!(matches!(
        format::write(&name, &parts),
        Err(InferError::Parts(_))
    ));
    let mut battle = parts;
    battle.battle = Some("{}".to_owned());
    assert!(matches!(
        format::write(&meta(), &battle),
        Err(InferError::Parts(_))
    ));
}

fn reviewer_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/reviewer/checkpoint-473312")
}

fn reviewer_files() -> (Vec<u8>, BTreeMap<String, Vec<u8>>) {
    let dir = reviewer_dir();
    let manifest = std::fs::read(dir.join("manifest.json")).unwrap();
    let files = [
        "trunk.safetensors",
        "readout.safetensors",
        "value.safetensors",
        "embedding.safetensors",
        "slots.json",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), std::fs::read(dir.join(name)).unwrap()))
    .collect();
    (manifest, files)
}

#[test]
fn the_committed_reviewer_bundle_packs_and_scores() {
    let parts = bundle::read(&reviewer_dir()).expect("the reviewer bundle reads");
    assert_eq!(parts.width, 256);
    assert_eq!(parts.capacity, 20_480);
    assert_eq!(parts.heads, LEGACY_HEADS);
    assert!(parts.blocks.is_empty());
    assert_eq!(parts.source.schema, 7);
    assert_eq!(parts.source.projection_abi, 2);
    {
        use sha2::Digest as _;
        let manifest = std::fs::read(reviewer_dir().join("manifest.json")).unwrap();
        assert_eq!(
            parts.source.manifest_sha256,
            format!("{:x}", sha2::Sha256::digest(manifest))
        );
    }

    let meta = Meta {
        name: "reviewer".to_owned(),
        label: "Reviewer 473312".to_owned(),
        temperature: 0.25,
    };
    let bytes = format::write(&meta, &parts).expect("packs");
    let model = Model::from_bytes(&bytes).expect("reads");
    assert_eq!(model.width(), 256);
    assert_eq!(model.slots_json(), parts.slots);

    let mut rng = Rng::new(473_312);
    let options = options(&mut rng, 7, parts.capacity);
    for (faction, head) in [(27, 3), (9, 12)] {
        let logits = model.forward(faction, head, &options).expect("forward");
        assert!(logits.iter().all(|logit| logit.is_finite()), "{logits:?}");
        assert_eq!(
            bits(&logits),
            bits(&reference(&parts, faction, head, &options))
        );
    }
}

#[test]
fn a_bundle_of_another_schema_or_with_a_changed_file_is_refused() {
    let (manifest, files) = reviewer_files();
    let mut document: Value = serde_json::from_slice(&manifest).unwrap();
    for schema in [6, 13] {
        document["schema"] = schema.into();
        let error = bundle::from_files(&serde_json::to_vec(&document).unwrap(), &BTreeMap::new())
            .unwrap_err();
        assert!(
            error.to_string().contains("unsupported MLP bundle schema"),
            "{error}"
        );
    }
    // Schema 8 needs residual blocks that this manifest does not declare.
    document["schema"] = 8.into();
    assert!(bundle::from_files(&serde_json::to_vec(&document).unwrap(), &files).is_err());

    let mut changed = files.clone();
    changed.get_mut("slots.json").unwrap()[10] ^= 1;
    let error = bundle::from_files(&manifest, &changed).unwrap_err();
    assert!(error.to_string().contains("hashes"), "{error}");

    let mut missing = files;
    missing.remove("value.safetensors");
    assert!(matches!(
        bundle::from_files(&manifest, &missing),
        Err(InferError::Bundle(_))
    ));
}

#[test]
fn safetensors_are_parsed_by_hand() {
    let header = br#"{"b":{"dtype":"F32","shape":[2],"data_offsets":[8,16]},"a":{"dtype":"F32","shape":[1,2],"data_offsets":[0,8]},"__metadata__":{"format":"pt"}}"#;
    let mut file = (header.len() as u64).to_le_bytes().to_vec();
    file.extend_from_slice(header);
    for value in [1.0f32, 2.0, 3.0, 4.0] {
        file.extend_from_slice(&value.to_le_bytes());
    }
    let tensors = bundle::parse_safetensors("t", &file).expect("parses");
    assert_eq!(tensors["a"].shape, vec![1, 2]);
    assert_eq!(tensors["a"].values, vec![1.0, 2.0]);
    assert_eq!(tensors["b"].values, vec![3.0, 4.0]);

    assert!(
        bundle::parse_safetensors("t", &file[..file.len() - 4]).is_err(),
        "short data"
    );
    let f16 = String::from_utf8(header.to_vec())
        .unwrap()
        .replacen("F32", "F16", 1);
    let mut other = (f16.len() as u64).to_le_bytes().to_vec();
    other.extend_from_slice(f16.as_bytes());
    other.extend_from_slice(&file[8 + header.len()..]);
    assert!(bundle::parse_safetensors("t", &other).is_err(), "not f32");
}

/// The requests of `fixtures/infer/synthetic_logits.json`: two models (widths 128 and 256, 2 and
/// 1 residual blocks) and decisions of 1 to 37 options.
fn synthetic_requests() -> Vec<Value> {
    let cases = |counts: &[usize]| -> Vec<Value> {
        counts
            .iter()
            .enumerate()
            .map(|(index, &count)| {
                serde_json::json!({
                    "seed": 100 + index,
                    "options": count,
                    "columns": 60,
                    "faction": (index * 7) % 33,
                    "head": (index * 5) % 14,
                })
            })
            .collect()
    };
    vec![
        serde_json::json!({
            "seed": 11, "width": 128, "capacity": 500, "blocks": 2,
            "cases": cases(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 13, 37]),
        }),
        serde_json::json!({
            "seed": 12, "width": 256, "capacity": 300, "blocks": 1,
            "cases": cases(&[1, 3, 6, 11]),
        }),
    ]
}

/// The logits of the synthetic models, which the wasm build must give bit for bit
/// (`web2/src/session/inferCheck.test.ts`, the forward on simd128). `cargo wire-web` writes the
/// fixture (this test, with `TI4_WRITE_WIRE`); otherwise the test fails while it differs.
#[test]
fn synthetic_logits_fixture_is_current() {
    let fixture: Vec<Value> = synthetic_requests()
        .into_iter()
        .map(|request| {
            let answer: Value =
                serde_json::from_str(&ti4_infer::synthetic::check(&request.to_string()).unwrap())
                    .unwrap();
            assert_eq!(answer["backend"], ti4_infer::backend());
            serde_json::json!({
                "request": request,
                "digest": answer["digest"],
                "logits": answer["logits"],
            })
        })
        .collect();
    let fixture = Value::Array(fixture);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/infer/synthetic_logits.json");
    if std::env::var_os("TI4_WRITE_WIRE").is_some_and(|value| value == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pretty = serde_json::to_string_pretty(&fixture).unwrap();
        std::fs::write(&path, format!("{pretty}\n")).unwrap();
        return;
    }
    let kept: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    assert!(
        kept == fixture,
        "fixtures/infer/synthetic_logits.json differs from the logits of the forward; write it \
         again with `cargo wire-web`"
    );
}

#[test]
fn a_check_request_of_another_shape_is_refused() {
    use ti4_infer::synthetic::check;
    assert!(check("not json").is_err());
    assert!(check(r#"{"seed":1,"width":128,"capacity":500,"blocks":0}"#).is_err());
    assert!(check(r#"{"seed":1,"width":128,"capacity":10,"blocks":0,"cases":[]}"#).is_err());
    assert!(check(r#"{"seed":1,"width":130,"capacity":50,"blocks":0,"cases":[]}"#).is_err());
    let one = r#"{"seed":1,"width":128,"capacity":50,"blocks":0,"cases":[{"seed":1,"options":2,"columns":5,"faction":33,"head":0}]}"#;
    assert!(check(one).is_err());
}

/// `ti4-pack` on the committed reviewer bundle writes a file that reads, with the name, the label
/// and the temperature it was given, and the bytes of an in-memory pack.
#[test]
fn the_pack_tool_writes_a_model_file_that_reads() {
    let dir = std::env::temp_dir().join(format!("ti4-pack-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("reviewer.ti4m");
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_ti4-pack"))
        .arg(reviewer_dir())
        .args(["reviewer"])
        .arg(&out)
        .args(["--label", "Reviewer 473312", "--temperature", "0.5"])
        .output()
        .expect("ti4-pack runs");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let bytes = std::fs::read(&out).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let model = Model::from_bytes(&bytes).expect("the packed file reads");
    assert_eq!(model.name(), "reviewer");
    assert_eq!(model.label(), "Reviewer 473312");
    assert_eq!(model.temperature(), 0.5);
    assert_eq!(model.width(), 256);
    assert!(stdout.contains(model.digest()), "{stdout}");
    let meta = Meta {
        name: "reviewer".to_owned(),
        label: "Reviewer 473312".to_owned(),
        temperature: 0.5,
    };
    let parts = bundle::read(&reviewer_dir()).unwrap();
    assert!(format::write(&meta, &parts).unwrap() == bytes);
}
