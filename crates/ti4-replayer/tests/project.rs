//! The branch tree and its project file.
//!
//! A project is a recipe, so almost all of these tests are about identity, inheritance, bounds and
//! refusal, with no engine involved. They are deliberately unflattering - half of them hand the
//! loader a file that a person edited by hand, because that is the case a persistence format is
//! judged on. The last test is the opposite: it plays a real branch, persists it, reads it back, and
//! rebuilds from the file, because a recipe that does not cook is not a recipe.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ti4_model::id::PlayerId;
use ti4_replayer::fingerprint::FrameFingerprint;
use ti4_replayer::live::{AdvanceGoal, LiveBranch, LiveState};
use ti4_replayer::project::{MAX_TOTAL_FRAMES, Origin};
use ti4_replayer::rebuild::{RebuildBounds, RebuildTarget, rebuild};
use ti4_replayer::{
    BranchId, ChoiceFingerprint, MAX_BRANCHES_PER_PROJECT, MAX_PROJECT_BYTES, OfferedOption,
    ProjectError, Provenance, ReplayInputs, ReplayRecord, ReplayerProject, SeatControl, SeatMode,
    SeatSetting, SourceTimeline, Verification, load_project, save_project, sha256_file,
};
use ti4_review::SimulationConfig;

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";

/// A directory that removes itself. Tests keep their artifacts well under the 32 MiB the package
/// allows: the largest file copied here is the 1 MiB semantic golden.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "ti4-r02-project-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary directory for a project file");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// How a test spoofs a project file it should refuse.
type Mutate = fn(&mut serde_json::Value);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<name> sits directly under the workspace root")
        .to_path_buf()
}

fn inputs() -> ReplayInputs {
    ReplayInputs {
        checkpoint: CHECKPOINT.to_owned(),
        map_pool: MAP_POOL.to_owned(),
        seed: 4_242,
        rotation: 1,
        profile_table: "learner".to_owned(),
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    }
}

fn source(frames: u64) -> SourceTimeline {
    SourceTimeline {
        session: "session.json".to_owned(),
        session_sha256: "a".repeat(64),
        checkpoint_sha256: "b".repeat(64),
        map_pool_sha256: "c".repeat(64),
        engine_commit: Some(ti4_review::ENGINE_COMMIT.to_owned()),
        content_sha256: Some(ti4_replayer::project::embedded_content()),
        frames,
        factions: ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
            .map(str::to_owned)
            .to_vec(),
    }
}

fn project(frames: u64) -> ReplayerProject {
    ReplayerProject::new(inputs(), source(frames))
}

fn option(id: &str) -> OfferedOption {
    OfferedOption {
        id: id.to_owned(),
        kind: "test".to_owned(),
        label: id.to_owned(),
        payload: BTreeMap::new(),
        preview: None,
        score: None,
        probability: None,
    }
}

fn record(branch: BranchId, frame: u64, ask: u32, chosen: &str) -> ReplayRecord {
    let actor = PlayerId::new("seat2");
    let offered = vec![option("a"), option("b"), option("c")];
    ReplayRecord {
        branch,
        frame,
        ask,
        actor: actor.clone(),
        faction: None,
        prompt: "Choose a thing".to_owned(),
        offered: offered.iter().map(|option| option.id.clone()).collect(),
        chosen: chosen.to_owned(),
        context: None,
        fingerprint: ChoiceFingerprint::compute(&actor, "Choose a thing", &offered, None),
        provenance: Provenance::Human,
    }
}

fn manual_seats() -> Vec<SeatSetting> {
    (0..6)
        .map(|seat| SeatSetting {
            player: format!("seat{seat}"),
            mode: SeatMode::Manual,
        })
        .collect()
}

fn verified() -> Verification {
    Verification {
        matches: true,
        engine_commit: ti4_review::ENGINE_COMMIT.to_owned(),
        engine_matches: true,
        content: ti4_replayer::project::embedded_content(),
    }
}

#[test]
fn a_new_project_carries_its_source_branch() {
    let project = project(10);
    assert_eq!(project.branches.len(), 1);
    let source = project.branch(BranchId::SOURCE).expect("branch-0");
    assert_eq!(source.parent, None);
    assert_eq!(source.origin, Origin::Imported);
    assert_eq!(source.frames, 10);
    assert!(source.answers.is_empty(), "the source replayed no choices");
    assert!(source.playable(&verified()));
    assert_eq!(project.total_frames(), 10);
    assert_eq!(project.next_branch, 1);
    project.validate().expect("a fresh project is valid");
}

#[test]
fn siblings_and_nests_share_one_parent() {
    let mut project = project(10);
    let first = project
        .fork(
            BranchId::SOURCE,
            4,
            Some("first".to_owned()),
            manual_seats(),
        )
        .expect("first child");
    let second = project
        .fork(BranchId::SOURCE, 7, None, Vec::new())
        .expect("second child");
    let nested = project
        .fork(first, 3, Some("grandchild".to_owned()), Vec::new())
        .expect("grandchild");
    assert_eq!(first.index(), 1);
    assert_eq!(second.index(), 2);
    assert_eq!(nested.index(), 3);
    assert_eq!(project.children_of(BranchId::SOURCE), vec![first, second]);
    assert_eq!(project.children_of(first), vec![nested]);
    assert_eq!(project.children_of(nested), vec![]);
    assert_eq!(
        project.path_to(nested).expect("a walkable chain"),
        vec![BranchId::SOURCE, first, nested]
    );
    // The parent chosen at creation is the parent forever: there is no setter, and the chain the
    // file records is what a later load reproduces.
    let source_branch = project.branch(BranchId::SOURCE).expect("branch-0");
    assert_eq!(source_branch.parent, None);
    assert_eq!(
        project.branch(first).expect("first").parent,
        Some(BranchId::SOURCE)
    );
    assert_eq!(project.branch(nested).expect("nested").parent, Some(first));
    assert_eq!(
        project.branch(first).expect("first").title.as_deref(),
        Some("first"),
        "titles are presentation and survive untouched"
    );
    assert_eq!(project.branch(first).expect("first").seats.len(), 6);
}

#[test]
fn branch_ids_are_monotonic_and_never_reused() {
    let mut project = project(4);
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.push(
            project
                .fork(BranchId::SOURCE, 1, None, Vec::new())
                .expect("a child"),
        );
    }
    assert_eq!(
        ids.iter().copied().map(BranchId::index).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(project.next_branch, 6);
    // Dropping a branch from the file must not hand its id to the next fork: a stale reference in a
    // saved GUI layout could then point at an unrelated timeline.
    project
        .branches
        .retain(|branch| branch.id != BranchId::new(3));
    let after = project
        .fork(BranchId::SOURCE, 1, None, Vec::new())
        .expect("a child after a gap");
    assert_eq!(after.index(), 6, "the freed id is not handed out again");
}

#[test]
fn a_project_holds_at_most_its_branch_bound() {
    let mut project = project(2);
    let mut count = 1;
    loop {
        match project.fork(BranchId::SOURCE, 1, None, Vec::new()) {
            Ok(_) => count += 1,
            Err(ProjectError::TooManyBranches { max }) => {
                assert_eq!(max, MAX_BRANCHES_PER_PROJECT);
                break;
            }
            Err(other) => panic!("unexpected refusal while filling the project: {other:?}"),
        }
    }
    assert_eq!(count, MAX_BRANCHES_PER_PROJECT);
    assert_eq!(project.branches.len(), MAX_BRANCHES_PER_PROJECT as usize);
    // The refusal must not have damaged the project it turned away.
    project.validate().expect("a full project is still valid");
}

#[test]
fn the_frame_budget_spans_every_branch() {
    // A near-full imported session: the fork below is legal against its own parent, and is refused
    // only because the tree it would join is already at R01's frame bound.
    let mut source = project(2);
    source.branches[0].frames = MAX_TOTAL_FRAMES - 1;
    let error = source
        .fork(BranchId::SOURCE, 500_000, None, Vec::new())
        .expect_err("the tree has no room left for another 500k frames");
    assert!(
        matches!(error, ProjectError::FramesExceeded { .. }),
        "got {error:?}"
    );
    assert_eq!(source.branches.len(), 1, "a refused fork adds nothing");

    // Now leave room, and check that growth on an existing branch is charged the same way.
    source.branches[0].frames = 500_000;
    let child = source
        .fork(BranchId::SOURCE, 10, None, Vec::new())
        .expect("a fork that fits comfortably");
    assert_eq!(source.total_frames(), 500_011);

    let error = source
        .extend(child, Vec::new(), MAX_TOTAL_FRAMES)
        .expect_err("growing a branch past the budget must be refused");
    assert!(
        matches!(error, ProjectError::FramesExceeded { max, .. } if max == MAX_TOTAL_FRAMES),
        "got {error:?}"
    );
    assert_eq!(
        source.branch(child).expect("the child is untouched").frames,
        11,
        "a refused grow leaves the branch alone"
    );

    // No single branch may exceed the budget either, even the imported source.
    let error = source
        .extend(BranchId::SOURCE, Vec::new(), MAX_TOTAL_FRAMES + 1)
        .expect_err("one over the budget is one over");
    assert!(
        matches!(error, ProjectError::FramesExceeded { .. }),
        "got {error:?}"
    );
    assert_eq!(
        source.total_frames(),
        500_011,
        "the budget is still what it was"
    );
}

#[test]
fn a_fork_cannot_reach_past_the_end_of_its_parent() {
    let mut project = project(10);
    let error = project
        .fork(BranchId::SOURCE, 10, None, Vec::new())
        .expect_err("frame 10 does not exist in a 10-frame branch");
    assert!(
        matches!(
            error,
            ProjectError::FrameBeyondBranch {
                wanted: 10,
                limit: 10,
                ..
            }
        ),
        "got {error:?}"
    );
    let error = project
        .fork(BranchId::new(77), 1, None, Vec::new())
        .expect_err("an unknown branch cannot be forked");
    assert!(
        matches!(error, ProjectError::UnknownBranch { .. }),
        "got {error:?}"
    );
}

#[test]
fn a_round_trip_survives_both_extensions() {
    for name in ["project.json", "project.json.zst"] {
        let temp = TempDir::new("round-trip");
        let path = temp.path.join(name);
        let mut original = project(10);
        let child = original
            .fork(
                BranchId::SOURCE,
                3,
                Some("what if seat0 said no".to_owned()),
                manual_seats(),
            )
            .expect("a child");
        original
            .extend(
                BranchId::SOURCE,
                vec![
                    record(BranchId::SOURCE, 1, 0, "a"),
                    record(BranchId::SOURCE, 2, 3, "c"),
                ],
                10,
            )
            .expect("record the source's own answers");
        original
            .extend(child, vec![record(child, 4, 0, "b")], 6)
            .expect("record the child's answer");
        original
            .mark_verified(child)
            .expect("the child has been reproduced");
        original
            .set_ui(
                child,
                Some(serde_json::json!({"frame": 4, "drawer": "events"})),
            )
            .expect("attach a UI selection");
        original
            .extend(
                BranchId::SOURCE,
                original
                    .branch(BranchId::SOURCE)
                    .expect("source")
                    .answers
                    .clone(),
                10,
            )
            .expect("resort the source records");

        save_project(&path, &original).expect("write the project");
        let loaded = load_project(&path).expect("read the project back");
        assert_eq!(loaded, original, "round trip through {name}");
        assert_eq!(
            loaded.expected_checksum(),
            loaded.checksum,
            "a freshly loaded project carries its own checksum"
        );
        assert!(
            fs::read_dir(&temp.path)
                .expect("list the directory")
                .filter_map(std::result::Result::ok)
                .all(|entry| {
                    let name = entry.file_name();
                    name.as_encoded_bytes() != b"project.json.tmp"
                        && name.as_encoded_bytes() != b"project.json.bak"
                }),
            "a successful save leaves no temporary files behind"
        );
    }
}

#[test]
fn an_interrupted_write_leaves_the_previous_project() {
    let temp = TempDir::new("interrupted");
    let path = temp.path.join("project.json");
    let mut original = project(10);
    save_project(&path, &original).expect("first write");
    let before = fs::read(&path).expect("read it back");

    // A write that cannot even create its temporary file - a full disk, a directory in the way -
    // must leave the project that was there readable.
    fs::create_dir(path.with_file_name("project.json.tmp")).expect("block the temporary path");
    original
        .fork(
            BranchId::SOURCE,
            2,
            Some("never persisted".to_owned()),
            Vec::new(),
        )
        .expect("a fork that will not reach the disk");
    let error = save_project(&path, &original).expect_err("the write should fail");
    assert!(matches!(error, ProjectError::Io { .. }), "got {error:?}");
    assert_eq!(fs::read(&path).expect("the old project"), before);
    let loaded = load_project(&path).expect("the old project still reads");
    assert_eq!(
        loaded.branches.len(),
        1,
        "the refused fork is not in the file"
    );

    // A refusal before any I/O must not touch the file either.
    fs::remove_dir(path.with_file_name("project.json.tmp")).expect("unblock the temporary path");
    let error = save_project(&path, &{
        let mut bloated = original.clone();
        bloated.branches[0].frames = MAX_TOTAL_FRAMES + 1;
        bloated
    })
    .expect_err("an out-of-budget project must be refused");
    assert!(
        matches!(error, ProjectError::FramesExceeded { .. }),
        "got {error:?}"
    );
    assert_eq!(fs::read(&path).expect("the old project"), before);
}

#[test]
fn a_malformed_project_is_refused_not_repaired() {
    let temp = TempDir::new("malformed");
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not json at all", b"{ not json ".to_vec()),
        (
            "an array where an object belongs",
            br#"[{"schema":"r02-replayer-project"}]"#.to_vec(),
        ),
        ("an empty file", Vec::new()),
        (
            "truncated mid-payload",
            br#"{"schema":"r02-replayer-project","version":1,"inputs":"#.to_vec(),
        ),
    ];
    for (what, bytes) in cases {
        let path = temp.path.join(format!("{}.json", what.replace(' ', "-")));
        fs::write(&path, &bytes).expect("write the file");
        let error = load_project(&path).expect_err(&format!("{what} must be refused"));
        assert!(
            matches!(
                error,
                ProjectError::Malformed { .. } | ProjectError::Unsupported { .. }
            ),
            "{what}: got {error:?}"
        );
    }

    // A compressed file that is not a valid stream is a read failure, not an empty project.
    let path = temp.path.join("broken.json.zst");
    fs::write(&path, b"this is not a zstd stream").expect("write the file");
    let error = load_project(&path).expect_err("a broken stream must be refused");
    assert!(matches!(error, ProjectError::Io { .. }), "got {error:?}");
}

#[test]
fn a_foreign_schema_or_version_is_refused() {
    let temp = TempDir::new("schema");
    let cases: Vec<(&str, Mutate)> = vec![
        ("another tool", |value: &mut serde_json::Value| {
            value["schema"] = serde_json::json!("ti4-review-session");
        }),
        (
            "a version from the future",
            |value: &mut serde_json::Value| value["version"] = serde_json::json!(99),
        ),
        (
            "a version from before this format",
            |value: &mut serde_json::Value| value["version"] = serde_json::json!(0),
        ),
        (
            "a field this build does not know",
            |value: &mut serde_json::Value| value["surprise"] = serde_json::json!(1),
        ),
    ];
    for (what, mutate) in cases {
        let path = temp.path.join(format!("{}.json", what.replace(' ', "-")));
        let mut value = serde_json::to_value(project(4)).expect("serialize a project");
        mutate(&mut value);
        fs::write(&path, serde_json::to_vec(&value).expect("serialize")).expect("write");
        let error = load_project(&path).expect_err(&format!("{what} must be refused"));
        assert!(
            matches!(
                error,
                ProjectError::Unsupported { .. } | ProjectError::Malformed { .. }
            ),
            "{what}: got {error:?}"
        );
    }
}

#[test]
fn a_tampered_payload_is_refused_even_when_it_stays_valid() {
    let temp = TempDir::new("checksum");
    let path = temp.path.join("project.json");
    let mut original = project(6);
    let child = original
        .fork(
            BranchId::SOURCE,
            2,
            Some("honest title".to_owned()),
            Vec::new(),
        )
        .expect("a child");
    original
        .mark_verified(child)
        .expect("the child is reproduced");
    save_project(&path, &original).expect("write");

    // Editing a title changes no rule the validator checks, so the checksum is the only thing that
    // can catch it - and "verified" is exactly the flag a tampered file must not be allowed to fake.
    let mut value = serde_json::to_value(&original).expect("serialize");
    value["branches"][1]["title"] = serde_json::json!("edited by hand");
    fs::write(&path, serde_json::to_vec(&value).expect("serialize")).expect("write");
    let error = load_project(&path).expect_err("an edited payload must be refused");
    assert!(
        matches!(error, ProjectError::Checksum { .. }),
        "got {error:?}"
    );

    // Same for unverifiable provenance: flip the recorded content digest and nothing may be live.
    let mut value = serde_json::to_value(&original).expect("serialize");
    value["source"]["content_sha256"] = serde_json::json!("0".repeat(64));
    fs::write(&path, serde_json::to_vec(&value).expect("serialize")).expect("write");
    let error = load_project(&path).expect_err("an edited digest must be refused");
    assert!(
        matches!(error, ProjectError::Checksum { .. }),
        "got {error:?}"
    );
}

#[test]
fn a_missing_parent_or_a_cycle_is_refused() {
    let temp = TempDir::new("graph");
    let cases: Vec<(&str, Mutate)> = vec![
        (
            "a parent that is not there",
            |value: &mut serde_json::Value| value["branches"][1]["parent"] = serde_json::json!(77),
        ),
        (
            "a branch that is its own parent",
            |value: &mut serde_json::Value| value["branches"][1]["parent"] = serde_json::json!(1),
        ),
        (
            "two branches that are each other's parent",
            |value: &mut serde_json::Value| {
                value["branches"][1]["parent"] = serde_json::json!(2);
                value["branches"][2]["parent"] = serde_json::json!(1);
            },
        ),
        (
            "a child older than its parent",
            |value: &mut serde_json::Value| {
                value["branches"][1]["parent"] = serde_json::json!(2);
                value["branches"][2]["parent"] = serde_json::json!(0);
            },
        ),
        ("the same id twice", |value: &mut serde_json::Value| {
            value["branches"][2]["id"] = serde_json::json!(1);
        }),
        (
            "an allocator behind the ids it handed out",
            |value: &mut serde_json::Value| value["next_branch"] = serde_json::json!(1),
        ),
        (
            "two parentless source branches",
            |value: &mut serde_json::Value| {
                value["branches"][2]["parent"] = serde_json::json!(null);
            },
        ),
        ("no branches at all", |value: &mut serde_json::Value| {
            value["branches"] = serde_json::json!([]);
        }),
        (
            "an answer that was never on offer",
            |value: &mut serde_json::Value| {
                value["branches"][0]["answers"] = serde_json::json!([{
                    "branch": 0,
                    "frame": 1,
                    "ask": 0,
                    "actor": "seat2",
                    "faction": null,
                    "prompt": "Choose a thing",
                    "offered": ["a", "b"],
                    "chosen": "z",
                    "context": null,
                    "fingerprint": format!("sha256:{}", "0".repeat(64)),
                    "provenance": "human",
                }]);
            },
        ),
        (
            "a fork frame past the end of its own branch",
            |value: &mut serde_json::Value| {
                value["branches"][1]["frames"] = serde_json::json!(1);
            },
        ),
    ];
    for (what, mutate) in cases {
        let path = temp.path.join(format!("{}.json", what.replace(' ', "-")));
        let mut value = serde_json::to_value(two_children()).expect("serialize a small tree");
        mutate(&mut value);
        fs::write(&path, serde_json::to_vec(&value).expect("serialize")).expect("write");
        let error = load_project(&path).expect_err(&format!("{what} must be refused"));
        assert!(
            matches!(
                error,
                ProjectError::MissingParent { .. }
                    | ProjectError::Cycle { .. }
                    | ProjectError::NotMonotonic { .. }
                    | ProjectError::BadSource { .. }
                    | ProjectError::FrameBeyondBranch { .. }
                    | ProjectError::Malformed { .. }
            ),
            "{what}: got {error:?}"
        );
    }
}

/// branch-0 with two children, so a test can tamper with either level.
fn two_children() -> ReplayerProject {
    let mut project = project(10);
    project
        .fork(BranchId::SOURCE, 4, None, Vec::new())
        .expect("first child");
    project
        .fork(BranchId::SOURCE, 5, None, Vec::new())
        .expect("second child");
    project
}

#[test]
fn reading_a_branch_never_mutates_the_project() {
    let mut original = two_children();
    original
        .extend(
            BranchId::SOURCE,
            vec![
                record(BranchId::SOURCE, 1, 0, "a"),
                record(BranchId::SOURCE, 6, 1, "b"),
            ],
            10,
        )
        .expect("record answers");
    let before_text = serde_json::to_string(&original).expect("serialize");
    let before_checksum = original.expected_checksum();

    // Every read path in the API, twice.
    for _ in 0..2 {
        let branch = original.branch(BranchId::new(1)).expect("child");
        assert_eq!(branch.frames, 5);
        assert_eq!(
            original.children_of(BranchId::SOURCE),
            vec![BranchId::new(1), BranchId::new(2)]
        );
        assert_eq!(
            original.path_to(BranchId::new(2)).expect("a chain"),
            vec![BranchId::SOURCE, BranchId::new(2)]
        );
        assert_eq!(
            frames_of(&original.prefix(BranchId::new(2)).expect("a prefix")),
            vec![(1, 0)]
        );
        assert_eq!(
            original
                .replay_script(BranchId::new(2))
                .expect("a script")
                .len(),
            1
        );
        assert_eq!(original.total_frames(), 21);
    }

    assert_eq!(
        serde_json::to_string(&original).expect("serialize"),
        before_text
    );
    assert_eq!(original.expected_checksum(), before_checksum);
}

#[test]
fn a_prefix_inherits_up_to_the_fork_and_not_beyond() {
    let mut original = project(10);
    original
        .extend(
            BranchId::SOURCE,
            vec![
                record(BranchId::SOURCE, 2, 0, "a"),
                record(BranchId::SOURCE, 2, 1, "b"),
                record(BranchId::SOURCE, 7, 0, "c"),
            ],
            10,
        )
        .expect("record the source's answers");
    assert_eq!(
        frames_of(
            &original
                .prefix(BranchId::SOURCE)
                .expect("the source's own prefix")
        ),
        vec![(2, 0), (2, 1), (7, 0)],
        "a branch's own prefix is everything it answered, in engine order"
    );

    let child = original
        .fork(BranchId::SOURCE, 5, None, manual_seats())
        .expect("a child forked at frame 5");
    assert_eq!(
        frames_of(&original.prefix(child).expect("the child's prefix")),
        vec![(2, 0), (2, 1)],
        "the child inherits what happened before frame 5 and none of what came after"
    );

    original
        .extend(
            child,
            vec![record(child, 6, 0, "a"), record(child, 5, 0, "c")],
            9,
        )
        .expect("the child answers frames 5 and 6 its own way");
    assert_eq!(
        frames_of(&original.prefix(child).expect("the child's prefix")),
        vec![(2, 0), (2, 1), (5, 0), (6, 0)],
        "the child's own answers follow the inherited ones, ordered by frame then ask"
    );

    let grand = original
        .fork(child, 6, None, Vec::new())
        .expect("a grandchild forked inside the child's own answers");
    assert_eq!(
        frames_of(&original.prefix(grand).expect("the grandchild's prefix")),
        vec![(2, 0), (2, 1), (5, 0), (6, 0)],
        "each ancestor is cut after the frame the next one forked at, because the answers stamped          with that frame are the ones that produced the position forked from"
    );
    let earlier = original
        .fork(child, 5, None, Vec::new())
        .expect("a second grandchild, forked a frame earlier");
    assert_eq!(
        frames_of(&original.prefix(earlier).expect("its prefix")),
        vec![(2, 0), (2, 1), (5, 0)],
        "and a fork one frame earlier stops one frame earlier"
    );

    let script = original
        .replay_script(grand)
        .expect("a script for the grandchild");
    assert_eq!(script.len(), 4);
    assert_eq!(
        script.records()[0].provenance,
        Provenance::Human,
        "provenance travels with the record"
    );
}

fn frames_of(records: &[ReplayRecord]) -> Vec<(u64, u32)> {
    records
        .iter()
        .map(|record| (record.frame, record.ask))
        .collect()
}

#[test]
fn only_a_verified_branch_against_matching_inputs_is_playable() {
    let mut original = project(10);
    let child = original
        .fork(BranchId::SOURCE, 3, None, Vec::new())
        .expect("a child");
    let good = verified();
    assert!(
        !original.branch(child).expect("child").playable(&good),
        "a fork nobody has reproduced yet is not live"
    );
    assert!(
        original
            .branch(BranchId::SOURCE)
            .expect("source")
            .playable(&good),
        "the imported source is live the moment the inputs verify"
    );
    original.mark_verified(child).expect("mark it");
    assert!(original.branch(child).expect("child").playable(&good));

    // A project whose inputs no longer match has nothing live in it, verified or not.
    let changed = Verification {
        matches: false,
        ..good.clone()
    };
    assert!(
        !original.branch(child).expect("child").playable(&changed)
            && !original
                .branch(BranchId::SOURCE)
                .expect("source")
                .playable(&changed),
        "a changed input must take every branch out of play"
    );
    // Marking an unknown branch verified is refused, and does not fabricate one.
    let error = original
        .mark_verified(BranchId::new(42))
        .expect_err("there is no branch-42");
    assert!(
        matches!(error, ProjectError::UnknownBranch { .. }),
        "got {error:?}"
    );
}

#[test]
fn an_import_hashes_what_it_is_given() {
    let root = workspace_root();
    for path in [CHECKPOINT, MAP_POOL] {
        assert!(
            root.join(path).is_file(),
            "{path} is missing from the checkout"
        );
    }
    let temp = TempDir::new("import");
    let config = SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 4_242,
        rotation: 1,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    // A session this build wrote, through R01's own save path: the import contract is about the
    // files, so the session under import has to be a real one.
    let mut review =
        ti4_review::LiveReview::start(&config).expect("start a short run to import afterwards");
    review.advance(ti4_review::AdvanceUnit::Step, 6);
    let frames = review.session.frames.len();
    let session = temp.path.join("reviewed.json");
    ti4_review::save_session(&session, &review.session).expect("write the session R01 produced");
    let project = ReplayerProject::import(&session, &config, &root, None, None)
        .expect("import the session this build just wrote");
    assert_eq!(project.branches.len(), 1);
    assert_eq!(project.inputs, ReplayInputs::of(&config));
    assert_eq!(
        project.source.checkpoint_sha256,
        sha256_file(&root.join(CHECKPOINT)).expect("hash the checkpoint")
    );
    assert_eq!(
        project.source.session_sha256,
        sha256_file(&session).expect("hash the session")
    );
    assert_eq!(
        project.source.engine_commit.as_deref(),
        Some(ti4_review::ENGINE_COMMIT)
    );
    assert_eq!(
        usize::try_from(project.source.frames).expect("a frame count that fits"),
        frames,
        "the project records the length of the session it imported"
    );
    assert!(
        project
            .verify_inputs(&root)
            .expect("the inputs the project names are still the inputs on disk")
            .matches
    );

    // A hash the caller was promised and did not get is a refusal at import time, before anything
    // is played - which is what lets the GUI say "Play" and mean it.
    let error = ReplayerProject::import(&session, &config, &root, None, Some(&"0".repeat(64)))
        .expect_err("a promised checkpoint hash that does not match must refuse");
    assert!(
        matches!(error, ProjectError::Checksum { .. }),
        "got {error:?}"
    );

    // A checkpoint swapped after the import is caught by the same check on the way back in.
    let swapped = temp.path.join("swapped-slots.json");
    let mut bytes = fs::read(root.join(CHECKPOINT)).expect("read the checkpoint");
    bytes.push(b'\n');
    fs::write(&swapped, &bytes).expect("write the swapped checkpoint");
    let mut tampered = project.clone();
    tampered.inputs.checkpoint = swapped.display().to_string();
    let error = tampered
        .verify_inputs(&root)
        .expect_err("a swapped checkpoint must be refused");
    assert!(
        matches!(&error, ProjectError::InputChanged { .. }),
        "got {error:?}"
    );

    // A different content corpus is a different game, so nothing may be played from it.
    let mut tampered = project.clone();
    tampered.source.content_sha256 = Some("0".repeat(64));
    let error = tampered
        .verify_inputs(&root)
        .expect_err("a different content corpus must be refused");
    assert!(
        matches!(&error, ProjectError::ProvenanceMismatch { what, .. } if *what == "content digest"),
        "got {error:?}"
    );

    // A missing input is its own error, not a hash mismatch.
    let mut tampered = project;
    tampered.inputs.map_pool = temp.path.join("nowhere.json").display().to_string();
    let error = tampered
        .verify_inputs(&root)
        .expect_err("a missing input must be refused");
    assert!(
        matches!(&error, ProjectError::MissingInput { .. }),
        "got {error:?}"
    );
}

#[test]
fn importing_something_that_is_not_a_session_is_refused() {
    let root = workspace_root();
    let temp = TempDir::new("bad-import");
    let session = temp.path.join("session.json");
    fs::write(&session, b"{ not a session ").expect("write junk");
    let config = SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 1,
        rotation: 0,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let error = ReplayerProject::import(&session, &config, &root, None, None)
        .expect_err("junk cannot be imported");
    assert!(
        matches!(error, ProjectError::Malformed { .. }),
        "got {error:?}"
    );

    let error = ReplayerProject::import(&temp.path.join("absent.json"), &config, &root, None, None)
        .expect_err("an absent session cannot be imported");
    assert!(
        matches!(
            error,
            ProjectError::Io { .. }
                | ProjectError::MissingInput { .. }
                | ProjectError::Malformed { .. }
        ),
        "got {error:?}"
    );
}

#[test]
fn the_byte_bound_is_r01_s_and_is_applied_before_writing() {
    assert_eq!(
        MAX_PROJECT_BYTES,
        ti4_review::MAX_SESSION_BYTES,
        "the project bound is R01's session bound, not a number invented here"
    );
    let temp = TempDir::new("bound");
    let path = temp.path.join("project.json");
    // A payload over the bound is refused before a byte is written; building a 1 GiB project in
    // memory to prove the write path would be the expensive way to test a comparison.
    let mut huge = project(2);
    let filler = "x".repeat(4_096);
    huge.branches[0].title = Some(filler.clone());
    huge.validate()
        .expect("a long title is still a valid project");
    save_project(&path, &huge).expect("a small project writes");
    assert!(fs::read(&path).expect("read it back").len() < MAX_PROJECT_BYTES);
}

/// Play a real branch, persist it as a project file, and read that file back as if the process had
/// restarted. Returns the inputs, the loaded project, one fingerprint per recorded frame, the
/// verification the loaded project reports, and the last frame index.
fn persisted_branch(
    temp: &TempDir,
    steps: usize,
) -> (
    SimulationConfig,
    ReplayerProject,
    Vec<FrameFingerprint>,
    Verification,
    u64,
) {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 4_242,
        rotation: 1,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let branch =
        LiveBranch::start(config.clone(), SeatControl::all_auto()).expect("spawn a branch");
    let deadline = Instant::now() + Duration::from_secs(120);
    while branch.gate().state() != LiveState::Ready {
        assert_ne!(
            branch.gate().state(),
            LiveState::Failed,
            "the branch failed to start"
        );
        assert!(Instant::now() < deadline, "the branch never became ready");
        std::thread::sleep(Duration::from_millis(5));
    }
    branch.gate().enable_recording();
    branch
        .gate()
        .run(AdvanceGoal::Steps(steps))
        .expect("the advance is accepted");
    let until = Instant::now() + Duration::from_secs(120);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the branch stalled");
    }
    let answers = branch.gate().records();
    let session = branch.into_session().expect("the session comes back");
    assert!(
        !answers.is_empty(),
        "{steps} steps must settle something to record"
    );
    let digests: Vec<FrameFingerprint> = session.frames.iter().map(FrameFingerprint::of).collect();
    let last = u64::try_from(session.frames.len() - 1).expect("a frame index that fits");

    let session_path = temp.path.join("reviewed.json");
    ti4_review::save_session(&session_path, &session).expect("write the session R01 produced");
    let mut project =
        ReplayerProject::import(&session_path, &config, &root, None, None).expect("import it");
    project
        .extend(BranchId::SOURCE, answers, last + 1)
        .expect("record the prefix the branch actually settled");
    let path = temp.path.join("project.json.zst");
    save_project(&path, &project).expect("write the project");

    let loaded = load_project(&path).expect("read the project back");
    assert_eq!(loaded, project, "the file holds the project it was given");
    let verification = loaded
        .verify_inputs(&root)
        .expect("the inputs the project names are still the inputs on disk");
    (config, loaded, digests, verification, last)
}

#[test]
fn a_saved_project_rebuilds_what_it_recorded() {
    let temp = TempDir::new("rebuild");
    let (config, project, digests, _verification, last) = persisted_branch(&temp, 12);
    let prefix = project
        .prefix(BranchId::SOURCE)
        .expect("the recorded prefix");
    assert!(!prefix.is_empty(), "nothing was lost in the file");

    let cancel = AtomicBool::new(false);
    let rebuilt = rebuild(
        &config,
        SeatControl::all_auto(),
        project
            .replay_script(BranchId::SOURCE)
            .expect("a script from the file"),
        &digests,
        RebuildTarget::Frame(last.saturating_sub(1)),
        RebuildBounds::default(),
        &cancel,
    )
    .expect("the loaded project rebuilds to its own target");
    assert!(
        rebuilt.replayed > 0,
        "the prefix was forced, not merely available"
    );
    for (index, frame) in rebuilt.review.session.frames.iter().enumerate() {
        assert_eq!(
            FrameFingerprint::of(frame),
            digests[index],
            "rebuilt frame {index} is not the frame the project recorded"
        );
    }
}

#[test]
fn only_a_reproduced_fork_becomes_live() {
    // The decision the GUI takes before it enables "Play from this frame": fork, rebuild, and only
    // then mark the branch verified.
    let temp = TempDir::new("fork-live");
    let (config, mut project, digests, verification, last) = persisted_branch(&temp, 8);
    let target = last.saturating_sub(1);
    let fork = project
        .fork(
            BranchId::SOURCE,
            target,
            Some("from the loaded file".to_owned()),
            manual_seats(),
        )
        .expect("fork at the frame the project reached");
    assert!(
        !project
            .branch(fork)
            .expect("the fork")
            .playable(&verification),
        "a fork is not live until something proves it"
    );
    assert!(
        !project
            .prefix(fork)
            .expect("the fork inherits the prefix")
            .is_empty(),
        "the fork replays from what the file recorded"
    );

    let cancel = AtomicBool::new(false);
    let rebuilt = rebuild(
        &config,
        SeatControl::all_auto(),
        project.replay_script(fork).expect("the fork's script"),
        &digests,
        RebuildTarget::Frame(target),
        RebuildBounds::default(),
        &cancel,
    )
    .expect("the fork rebuilds to its own fork frame");
    assert_eq!(rebuilt.frames, usize::try_from(target).expect("fits") + 1);
    project.mark_verified(fork).expect("the rebuild proved it");
    assert!(
        project
            .branch(fork)
            .expect("the fork")
            .playable(&verification)
    );
}

/// A recording's own decisions are the answers branch-0 stands on, and importing them has to leave a
/// file this build can open.
///
/// Before this test, an imported project carried no answers at all: `Play from this frame` would have
/// rebuilt a prefix out of nothing, which is not a replay of the game on the screen. And the first
/// version of the import wrote the answers straight into the branch, leaving the checksum of the
/// answer-less project in the file - so the import could not be reopened at all. Both are checked here.
#[test]
fn an_imported_recording_carries_its_own_answers_and_survives_the_round_trip() {
    let temp = TempDir::new("imported-answers");
    let (session_path, settled) = record_a_game(&temp, 5);
    let root = workspace_root();
    let loaded = ti4_review::load_session(&session_path).expect("reload the recording");
    let inputs = ReplayInputs::from_manifest(&loaded.manifest);
    let project = ReplayerProject::import_with(&session_path, &loaded, &inputs, &root, None, None)
        .expect("import the recording");

    let answers = &project.branch(BranchId::SOURCE).expect("branch-0").answers;
    assert!(
        !answers.is_empty(),
        "the decisions the recording settled become the branch's answers"
    );
    for settled in &settled {
        assert!(
            answers.iter().any(|record| {
                record.frame == settled.frame
                    && record.actor == settled.actor
                    && record.chosen == settled.chosen
            }),
            "the recording's decision at frame {} by {} on {} is in the import",
            settled.frame,
            settled.actor,
            settled.chosen
        );
    }
    assert!(
        answers
            .iter()
            .all(|record| record.branch == BranchId::SOURCE),
        "an imported answer belongs to the recording, not to some branch that has not been made yet"
    );
    assert!(
        !project
            .replay_script(BranchId::SOURCE)
            .expect("a script for the recording")
            .is_empty(),
        "the import is reproducible: it has a prefix to force"
    );

    // The checksum regression: this is the file the window would be asked to open next session.
    let project_path = temp.path.join("imported.r02.json");
    save_project(&project_path, &project).expect("save the import");
    let reopened =
        load_project(&project_path).expect("the import reads back with its own checksum");
    assert_eq!(
        reopened
            .branch(BranchId::SOURCE)
            .expect("branch-0")
            .answers
            .len(),
        answers.len(),
        "every answer survives the file"
    );
}

/// A recording that claims a decision was settled on an option it never offered is refused outright.
#[test]
fn a_recording_that_settles_on_an_option_it_never_offered_is_not_a_recipe() {
    let temp = TempDir::new("doctored-answers");
    let (session_path, _settled) = record_a_game(&temp, 3);
    let mut loaded = ti4_review::load_session(&session_path).expect("reload the recording");
    let doctored = loaded
        .frames
        .iter_mut()
        .flat_map(|frame| frame.decisions.iter_mut())
        .find(|decision| !decision.options.is_empty())
        .expect("the recording holds a decision with options");
    doctored.chosen = Some("an-option-nobody-offered".to_owned());
    let error = ti4_replayer::project::answers_from_session(&loaded)
        .expect_err("a settled option that was never offered is not an answer");
    assert!(
        matches!(error, ProjectError::Malformed { .. }),
        "a prefix built from this would look legal while replaying an impossible game: {error}"
    );
}

/// Play a short real game with a recording gate and write it out as an R01 session file.
///
/// Returns the path and the gate's own records, which are the ground truth an honest import must
/// reproduce.
fn record_a_game(temp: &TempDir, steps: usize) -> (PathBuf, Vec<ReplayRecord>) {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 4_242,
        rotation: 1,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let branch =
        LiveBranch::start(config.clone(), SeatControl::all_auto()).expect("spawn a branch");
    let deadline = Instant::now() + Duration::from_secs(120);
    while branch.gate().state() != LiveState::Ready {
        assert_ne!(
            branch.gate().state(),
            LiveState::Failed,
            "the branch failed to start"
        );
        assert!(Instant::now() < deadline, "the branch never became ready");
        std::thread::sleep(Duration::from_millis(5));
    }
    branch.gate().enable_recording();
    branch
        .gate()
        .run(AdvanceGoal::Steps(steps))
        .expect("the advance is accepted");
    let until = Instant::now() + Duration::from_secs(180);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the branch stalled");
    }
    let settled = branch.gate().records();
    let session = branch.into_session().expect("the session comes back");
    let path = temp.path.join("reviewed.ti4review.json");
    ti4_review::save_session(&path, &session).expect("write the session R01 produced");
    (path, settled)
}

/// A table started in the window has a project before any recording exists.
///
/// The button is "Load starting table", the same words the reviewer uses, and what it must not require
/// is a file: nobody has recorded anything yet, because the operator is at the table. So the project
/// says what the table *is* - inputs, checkpoint and map-pool hashes, content corpus - and leaves the
/// session path empty rather than inventing one.
#[test]
fn a_live_table_is_a_project_before_it_is_a_recording() {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 4_242,
        rotation: 1,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    // The reviewer's own opening session supplies the seating and the content digest, which is all of
    // it the project needs from a game that has not advanced a step.
    let opening = ti4_review::LiveReview::start(&config)
        .expect("a starting table")
        .session;
    let inputs = ReplayInputs::of(&config);
    let project = ReplayerProject::live_table(&inputs, &root, &opening).expect("a live table");
    assert!(
        project.source.session.is_empty(),
        "nothing has been recorded, so the source names no file"
    );
    assert_eq!(project.source.frames, 0, "the table has not played yet");
    assert!(
        project.source.engine_commit.is_some(),
        "a table played here knows which build is playing it"
    );
    let verification = project
        .verify_inputs(&root)
        .expect("the checkpoint and pool are where the form said");
    assert!(verification.matches);
    assert!(
        verification.engine_matches,
        "this build is playing its own table"
    );
    let branch = project.branch(BranchId::SOURCE).expect("branch-0");
    assert!(
        branch.playable(&verification),
        "the table is playable from the first frame; nothing needs rebuilding"
    );
    assert!(
        project
            .replay_script(BranchId::SOURCE)
            .expect("a script")
            .is_empty(),
        "there is no prefix to force on a table that has played nothing"
    );

    // And it survives the file, because Save is how a table becomes somebody else's fork point.
    let temp = TempDir::new("live-table");
    let path = temp.path.join("table.r02.json");
    save_project(&path, &project).expect("save the table's project");
    let reopened = load_project(&path).expect("read it back");
    assert_eq!(
        reopened.branch(BranchId::SOURCE).expect("branch-0").origin,
        Origin::Live,
        "the file still says this branch was played, not imported"
    );
}
