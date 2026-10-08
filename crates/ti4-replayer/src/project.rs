//! The branch tree, and the project file that holds it.
//!
//! A replayer project is the *recipe* for a family of timelines, never a stack of saved games. It
//! names immutable inputs (checkpoint, map pool, seed, rotation, profile table, temperature,
//! diplomacy), the imported R01 session they produced, and a tree of branches. Each branch stores
//! the decisions made on it as [`ReplayRecord`]s — the same records [`crate::rebuild`] consumes — so
//! a branch is reproducible rather than restorable, and a project stays small enough to keep in a
//! working directory.
//!
//! # What the format refuses
//!
//! Branch ids are monotonic and never reused; a parent is chosen at creation and never changes; the
//! source timeline is always present, always id 0, and always parentless. Parents are always lower
//! ids than their children, which is what makes a cycle impossible to express — and a project file
//! that claims one anyway is refused, not repaired. The branch count is capped by
//! [`crate::control::MAX_BRANCHES_PER_PROJECT`], the summed frame count by R01's own frame bound, and
//! the file itself by R01's session bound, all checked before anything is written.
//!
//! # Verification is not a flag you can set on a hunch
//!
//! A branch is *live* — playable, in the GUI's sense — only when the inputs it names are still the
//! inputs on disk and a rebuild has actually matched. [`ReplayerProject::verify_inputs`] recomputes
//! the file hashes and compares the recorded engine and content commits against the ones this binary
//! was built with; [`Branch::playable`] then says whether a given branch may be opened. Import and
//! Play both go through the same check, so a checkpoint swapped since the import is caught before
//! the first step, not after a plausible-looking divergence.

use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::control::{
    BranchId, ChoiceFingerprint, MAX_BRANCHES_PER_PROJECT, OfferedOption, Provenance, ReplayRecord,
    SeatMode,
};
use crate::persistence::hex_digest;
use crate::persistence::sha256_path;
use crate::rebuild::ReplayScript;

/// Schema name stored in every project file.
pub const PROJECT_SCHEMA: &str = "r02-replayer-project";
/// Version of the project format described here.
pub const PROJECT_VERSION: u32 = 1;
/// Frames across all branches of one project, inherited from R01 rather than reinvented.
pub const MAX_TOTAL_FRAMES: u64 = ti4_review::MAX_FRAMES as u64;

/// Everything that can stop a project from being loaded, grown, or written.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// The file is not project JSON at all.
    #[error("replayer project is malformed: {detail}")]
    Malformed {
        /// What the parser or validator objected to.
        detail: String,
    },
    /// A file from another schema or version, which this build has no business interpreting.
    #[error(
        "unsupported replayer project {schema} v{version}, expected {PROJECT_SCHEMA} v{PROJECT_VERSION}"
    )]
    Unsupported {
        /// Schema name found in the file.
        schema: String,
        /// Version found in the file.
        version: u32,
    },
    /// The stored checksum does not match the payload.
    #[error("replayer project checksum is {found}, expected {expected}")]
    Checksum {
        /// Digest computed over the loaded payload.
        expected: String,
        /// Digest the file claimed.
        found: String,
    },
    /// The file is bigger than the bound this build will accept.
    #[error("replayer project is {bytes} bytes, over the {max}-byte limit")]
    TooLarge {
        /// Size in bytes.
        bytes: usize,
        /// The bound.
        max: usize,
    },
    /// More branches than the project may hold.
    #[error("replayer project holds its full {max} branches")]
    TooManyBranches {
        /// The bound.
        max: u32,
    },
    /// Branch frames add up past R01's frame bound.
    #[error("replayer project replays {total} frames, over the {max}-frame limit")]
    FramesExceeded {
        /// Summed frame count.
        total: u64,
        /// The bound.
        max: u64,
    },
    /// A branch names a parent the project does not contain.
    #[error("branch {branch} names parent {parent}, which the project does not contain")]
    MissingParent {
        /// The branch with the bad link.
        branch: BranchId,
        /// The id it names.
        parent: BranchId,
    },
    /// A parent chain that does not terminate, or a child older than its parent.
    #[error("branch {branch} is part of a parent cycle")]
    Cycle {
        /// A branch on the cycle.
        branch: BranchId,
    },
    /// Two branches share an id, or an id is not below the allocator's next value.
    #[error("replayer project branch ids are not monotonic: {detail}")]
    NotMonotonic {
        /// What was found.
        detail: String,
    },
    /// There is no single parentless source timeline at id 0.
    #[error("replayer project needs exactly one parentless source branch at branch-0: {detail}")]
    BadSource {
        /// What was found instead.
        detail: String,
    },
    /// A branch the project does not contain.
    #[error("replayer project has no {branch}")]
    UnknownBranch {
        /// The branch asked for.
        branch: BranchId,
    },
    /// A fork frame past the end of the branch being forked.
    #[error("{branch} ends at frame {}, so it cannot fork at frame {wanted}", .limit.saturating_sub(1))]
    FrameBeyondBranch {
        /// The branch asked to be forked.
        branch: BranchId,
        /// Frames that branch covers.
        limit: u64,
        /// The frame asked for.
        wanted: u64,
    },
    /// An input file the project names is not there.
    #[error("replayer project input {} is missing", .path.display())]
    MissingInput {
        /// The path that could not be read.
        path: std::path::PathBuf,
    },
    /// An input file changed since the project recorded it.
    #[error("replayer project input {} changed: recorded {expected}, found {found}", .path.display())]
    InputChanged {
        /// The path that no longer matches.
        path: std::path::PathBuf,
        /// Digest recorded at import.
        expected: String,
        /// Digest computed now.
        found: String,
    },
    /// The engine or content that made the source is not the engine or content running now.
    #[error("{} does not match: project recorded {expected}, this build reports {found}", .what)]
    ProvenanceMismatch {
        /// Which commitment disagrees.
        what: &'static str,
        /// What the project recorded.
        expected: String,
        /// What this build reports.
        found: String,
    },
    /// A file-system failure, kept separate from semantic refusals.
    #[error("the file system refused {} because {}", .path.display(), .source)]
    Io {
        /// The path involved.
        path: std::path::PathBuf,
        /// The operating-system error.
        source: std::io::Error,
    },
}

/// The inputs that make a replay reproducible: the same seven knobs R01 takes to start a run.
///
/// Stored as text because a project is a portable file, and resolved against a base directory when
/// the project is loaded. Absolute paths are kept as written; relative ones are re-based, so a
/// project moved between machines still resolves as long as the relative layout holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayInputs {
    pub checkpoint: String,
    pub map_pool: String,
    pub seed: u64,
    pub rotation: u32,
    /// `learner` or `expert`, as R01 spells them.
    pub profile_table: String,
    pub temperature: f64,
    pub diplomacy: bool,
    /// The six factions rotated, when not the standard lineup. Projects written before this
    /// existed played the standard lineup.
    #[serde(default)]
    pub lineup: Option<Vec<String>>,
}

impl ReplayInputs {
    /// The inputs a live run used, spelled the way a project spells them.
    #[must_use]
    pub fn of(config: &ti4_review::SimulationConfig) -> Self {
        Self {
            checkpoint: config.checkpoint.display().to_string(),
            map_pool: config.map_pool.display().to_string(),
            seed: config.seed,
            rotation: u32::try_from(config.rotation).unwrap_or(u32::MAX),
            profile_table: match config.table {
                ti4_review::ProfileTable::Learner => "learner",
                ti4_review::ProfileTable::Accepted => "accepted",
            }
            .to_owned(),
            temperature: config.temperature,
            diplomacy: config.diplomacy,
            lineup: config.lineup.clone(),
        }
    }

    /// The inputs an R01 recording was made with, read off its own manifest.
    ///
    /// This is the honest source for a replayer that was handed a session file and nothing else: the
    /// manifest says which checkpoint, pool, seed, rotation, table and temperature produced those
    /// frames, so the operator never has to remember or retype them - and if any of them is wrong the
    /// hashes recorded alongside it will say so at import.
    #[must_use]
    pub fn from_manifest(manifest: &ti4_review::SessionManifest) -> Self {
        Self {
            checkpoint: manifest.checkpoint_path.clone(),
            map_pool: manifest.map_pool_path.clone(),
            seed: manifest.seed,
            rotation: u32::try_from(manifest.rotation).unwrap_or(u32::MAX),
            profile_table: match manifest.profile_table {
                ti4_review::ProfileTable::Learner => "learner",
                ti4_review::ProfileTable::Accepted => "accepted",
            }
            .to_owned(),
            temperature: manifest.temperature,
            diplomacy: manifest.diplomacy,
            lineup: manifest.lineup.clone(),
        }
    }

    /// The simulation configuration these inputs describe, resolved against `base`.
    ///
    /// `None` when the profile table is spelled a way this build does not know: guessing a table
    /// would replay a different policy than the recording, which is the one thing a replayer must
    /// not do quietly.
    #[must_use]
    pub fn simulation_config(&self, base: &Path) -> Option<ti4_review::SimulationConfig> {
        let (checkpoint, map_pool) = self.paths(base);
        let table = self.table()?;
        Some(ti4_review::SimulationConfig {
            checkpoint,
            map_pool,
            seed: self.seed,
            rotation: usize::try_from(self.rotation).unwrap_or(usize::MAX),
            table,
            temperature: self.temperature,
            diplomacy: self.diplomacy,
            lineup: self.lineup.clone(),
        })
    }

    /// The two input files this project depends on, resolved against `base`.
    #[must_use]
    pub fn paths(&self, base: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        (rebase(base, &self.checkpoint), rebase(base, &self.map_pool))
    }

    /// The profile table this project names. `None` for a spelling this build does not know, which
    /// is a reason to refuse the project rather than guess a table.
    #[must_use]
    pub fn table(&self) -> Option<ti4_review::ProfileTable> {
        match self.profile_table.as_str() {
            "learner" => Some(ti4_review::ProfileTable::Learner),
            "accepted" => Some(ti4_review::ProfileTable::Accepted),
            _ => None,
        }
    }
}

fn rebase(base: &Path, stored: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(stored);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

/// The imported timeline a project grew out of, and the hashes that pin it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTimeline {
    /// The R01 session file this project was imported from.
    pub session: String,
    /// SHA-256 of that file at import.
    pub session_sha256: String,
    /// SHA-256 of the checkpoint file at import.
    pub checkpoint_sha256: String,
    /// SHA-256 of the map pool file at import.
    pub map_pool_sha256: String,
    /// Engine commit recorded by R01 for the source run.
    #[serde(default)]
    pub engine_commit: Option<String>,
    /// Content digest recorded by R01 for the source run.
    #[serde(default)]
    pub content_sha256: Option<String>,
    /// Frames the source session holds.
    pub frames: u64,
    /// Seating order (faction names) as the source recorded it.
    pub factions: Vec<String>,
}

/// Where a branch came from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Origin {
    /// The imported R01 session itself.
    Imported,
    /// A table this build started and is playing: no recording of it existed when the project was
    /// made, because the operator was at the table rather than behind it.
    Live,
    /// Forked from the parent at a frame boundary: everything strictly before `frame` is shared,
    /// and everything from `frame` on is the child's to answer.
    Fork { frame: u64 },
}

/// One seat's control mode on a branch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatSetting {
    pub player: String,
    pub mode: SeatMode,
}

/// One timeline in the tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
    pub id: BranchId,
    /// `None` for the source branch only.
    pub parent: Option<BranchId>,
    pub origin: Origin,
    /// Presentation. Never part of replay identity.
    #[serde(default)]
    pub title: Option<String>,
    /// Seat modes as this branch wants them at its start.
    #[serde(default)]
    pub seats: Vec<SeatSetting>,
    /// Decisions made *on this branch*, in `(frame, ask)` order. The full prefix a rebuild needs is
    /// this plus the inherited part of the parent chain; see [`ReplayerProject::prefix`].
    #[serde(default)]
    pub answers: Vec<ReplayRecord>,
    /// Frames this branch's timeline covers, as last rebuilt. Charged against the frame budget.
    pub frames: u64,
    /// Whether a rebuild has actually reproduced this branch's frames. Set by the code that
    /// replays, never by a caller who merely intends to.
    pub verified: bool,
    /// Optional UI selection (viewed frame, open drawer, scroll), preserved without interpretation.
    #[serde(default)]
    pub ui: Option<Value>,
}

impl Branch {
    /// The frame this branch diverges at, or `None` for the imported source.
    #[must_use]
    pub fn fork_frame(&self) -> Option<u64> {
        match self.origin {
            Origin::Imported | Origin::Live => None,
            Origin::Fork { frame } => Some(frame),
        }
    }

    /// Whether this branch may be played. A branch that has never been reproduced, or whose project
    /// has not been verified against the files on disk, is inspectable and not playable.
    #[must_use]
    pub fn playable(&self, verified: &Verification) -> bool {
        self.verified && verified.matches
    }
}

/// Proof that the project's inputs are still the inputs on disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Verification {
    /// Every hash that a replay depends on matched the file it names.
    pub matches: bool,
    /// Engine commit this build reports.
    pub engine_commit: String,
    /// Whether the source was made by this engine commit. Surfaced, not enforced: R01 stamps the
    /// repository commit of the build that ran it, so refusing on that field would reject every
    /// session made before the most recent commit while proving nothing about replay. The content
    /// digest and the input file hashes are what replay actually depends on, and those are enforced.
    pub engine_matches: bool,
    /// Content digest this build plays with.
    pub content: String,
}

/// A replayer project: inputs, source timeline, and the branch tree grown from it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayerProject {
    pub schema: String,
    pub version: u32,
    pub inputs: ReplayInputs,
    pub source: SourceTimeline,
    pub branches: Vec<Branch>,
    /// One past the highest id ever allocated, so ids are never reused even after a branch is
    /// dropped from the file.
    pub next_branch: u32,
    /// SHA-256 over the payload with this field blank.
    pub checksum: String,
}

impl ReplayerProject {
    /// A project holding only its source timeline.
    #[must_use]
    pub fn new(inputs: ReplayInputs, source: SourceTimeline) -> Self {
        Self::with_origin(inputs, source, Origin::Imported)
    }

    fn with_origin(inputs: ReplayInputs, source: SourceTimeline, origin: Origin) -> Self {
        let mut project = Self {
            schema: PROJECT_SCHEMA.to_owned(),
            version: PROJECT_VERSION,
            inputs,
            source,
            branches: Vec::new(),
            next_branch: 1,
            checksum: String::new(),
        };
        project.branches.push(Branch {
            id: BranchId::SOURCE,
            parent: None,
            origin,

            title: None,
            seats: Vec::new(),
            answers: Vec::new(),
            frames: project.source.frames,
            verified: true,
            ui: None,
        });
        project.restyle();
        project
    }

    /// A project for a table this build is playing right now.
    ///
    /// Every other project is a memory: it starts from an R01 recording and inherits its hashes. A
    /// live table has no recording yet - somebody sat down and started playing - so the source
    /// timeline states what the table *is* (inputs, checkpoint and map-pool hashes, content corpus,
    /// seating) and leaves the session file empty because it has not been written. "Save recording"
    /// in the window supplies it afterwards. Branch-0 is `verified` like any root: the table is not a
    /// claim about somebody else's game, it is the game, in this process, right now.
    ///
    /// # Errors
    /// [`ProjectError::MissingInput`] or [`ProjectError::Io`] when the checkpoint or map pool cannot
    /// be read, and [`ProjectError::ProvenanceMismatch`] when the content corpus differs.
    pub fn live_table(
        inputs: &ReplayInputs,
        base: &Path,
        session: &ti4_review::ReviewSession,
    ) -> Result<Self, ProjectError> {
        let inputs = inputs.clone();
        let (checkpoint, map_pool) = inputs.paths(base);
        let source = SourceTimeline {
            session: String::new(),
            session_sha256: String::new(),
            checkpoint_sha256: sha256_path(&checkpoint)?,
            map_pool_sha256: sha256_path(&map_pool)?,
            engine_commit: Some(ti4_review::ENGINE_COMMIT.to_owned()),
            content_sha256: session.manifest.content_sha256.clone(),
            frames: 0,
            factions: session.manifest.factions.clone(),
        };
        require_content(source.content_sha256.as_deref())?;
        Ok(Self::with_origin(inputs, source, Origin::Live))
    }

    /// Import an R01 session as the source timeline of a new project.
    ///
    /// The session is loaded through R01's own loader, so its schema, bounds and lineup are validated
    /// before anything is copied. The three input files are hashed here and the digests stored, and
    /// the engine commit recorded in the session must be the one this build reports.
    ///
    /// # Errors
    /// [`ProjectError::Io`]/[`ProjectError::MissingInput`] when a named file cannot be read,
    /// [`ProjectError::Malformed`] when R01 rejects the session, [`ProjectError::Checksum`] when an
    /// input hash disagrees with a hash the caller already had, and
    /// [`ProjectError::ProvenanceMismatch`] when the source's content corpus is not the one this
    /// build plays with.
    #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
    pub fn import(
        session_path: &Path,
        config: &ti4_review::SimulationConfig,
        base: &Path,
        expected_session_sha256: Option<&str>,
        expected_checkpoint_sha256: Option<&str>,
    ) -> Result<Self, ProjectError> {
        let session =
            ti4_review::load_session(session_path).map_err(|error| ProjectError::Malformed {
                detail: error.to_string(),
            })?;
        Self::import_with(
            session_path,
            &session,
            &ReplayInputs::of(config),
            base,
            expected_session_sha256,
            expected_checkpoint_sha256,
        )
    }

    /// Import a session that has already been loaded, from inputs the caller already holds.
    ///
    /// A replayer holds the session from the moment it opens one - the frames are what it draws - and
    /// a session is hundreds of megabytes, so loading it a second time inside [`import`] would double
    /// the wait and the footprint for nothing. This is the same import with the load hoisted out.
    ///
    /// # Errors
    /// As [`import`](Self::import).
    pub fn import_with(
        session_path: &Path,
        session: &ti4_review::ReviewSession,
        inputs: &ReplayInputs,
        base: &Path,
        expected_session_sha256: Option<&str>,
        expected_checkpoint_sha256: Option<&str>,
    ) -> Result<Self, ProjectError> {
        let inputs = inputs.clone();
        let (checkpoint, map_pool) = inputs.paths(base);
        let checkpoint_sha256 = sha256_path(&checkpoint)?;
        let map_pool_sha256 = sha256_path(&map_pool)?;
        let session_sha256 = sha256_path(session_path)?;
        for (want, got) in [
            (expected_checkpoint_sha256, checkpoint_sha256.as_str()),
            (expected_session_sha256, session_sha256.as_str()),
        ] {
            if let Some(want) = want
                && want != got
            {
                return Err(ProjectError::Checksum {
                    expected: want.to_owned(),
                    found: got.to_owned(),
                });
            }
        }
        require_content(session.manifest.content_sha256.as_deref())?;
        let frames = u64::try_from(session.frames.len()).unwrap_or(u64::MAX);
        let source = SourceTimeline {
            session: session_path.display().to_string(),
            session_sha256,
            checkpoint_sha256,
            map_pool_sha256,
            engine_commit: session.manifest.engine_commit.clone(),
            content_sha256: session.manifest.content_sha256.clone(),
            frames,
            factions: session.manifest.factions.clone(),
        };
        let mut project = Self::new(inputs, source);
        // Through `extend`, not by writing the fields: that is what re-runs the frame budget and, more
        // importantly, recomputes the checksum over what the branch now holds. Assigning the answers
        // directly left the file carrying the checksum of an answer-less branch, so the import could not
        // be opened by the build that wrote it - which is exactly how this line came to exist.
        let answers = answers_from_session(session)?;
        project.extend(BranchId::SOURCE, answers, frames)?;
        Ok(project)
    }

    /// Re-check every recorded hash against the files, and the recorded engine commit against this
    /// build. Returns `Err` on the first disagreement, which is exactly what the GUI needs before
    /// it enables Play from a frame.
    ///
    /// # Errors
    /// [`ProjectError::MissingInput`], [`ProjectError::InputChanged`] or
    /// [`ProjectError::ProvenanceMismatch`] (a different content corpus).
    pub fn verify_inputs(&self, base: &Path) -> Result<Verification, ProjectError> {
        let (checkpoint, map_pool) = self.inputs.paths(base);
        let found = sha256_path(&checkpoint)?;
        if found != self.source.checkpoint_sha256 {
            return Err(ProjectError::InputChanged {
                path: checkpoint,
                expected: self.source.checkpoint_sha256.clone(),
                found,
            });
        }
        let found = sha256_path(&map_pool)?;
        if found != self.source.map_pool_sha256 {
            return Err(ProjectError::InputChanged {
                path: map_pool,
                expected: self.source.map_pool_sha256.clone(),
                found,
            });
        }
        require_content(self.source.content_sha256.as_deref())?;
        let engine_commit = ti4_review::ENGINE_COMMIT.to_owned();
        let engine_matches = self
            .source
            .engine_commit
            .as_ref()
            .is_none_or(|recorded| recorded == &engine_commit);
        Ok(Verification {
            matches: true,
            engine_commit,
            engine_matches,
            content: embedded_content(),
        })
    }

    #[must_use]
    pub fn branch(&self, id: BranchId) -> Option<&Branch> {
        self.branches.iter().find(|branch| branch.id == id)
    }

    /// Direct children of a branch, in id order.
    #[must_use]
    pub fn children_of(&self, id: BranchId) -> Vec<BranchId> {
        let mut children: Vec<BranchId> = self
            .branches
            .iter()
            .filter(|branch| branch.parent == Some(id))
            .map(|branch| branch.id)
            .collect();
        children.sort_unstable();
        children
    }

    /// The chain from the source branch down to `id`, source first.
    ///
    /// # Errors
    /// [`ProjectError::UnknownBranch`] if `id` is not in the project, and
    /// [`ProjectError::MissingParent`] / [`ProjectError::Cycle`] if the parent chain cannot be
    /// walked.
    pub fn path_to(&self, id: BranchId) -> Result<Vec<BranchId>, ProjectError> {
        let mut chain = vec![id];
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(id);
        let mut current = id;
        loop {
            let Some(branch) = self.branch(current) else {
                return Err(ProjectError::UnknownBranch { branch: current });
            };
            let Some(parent) = branch.parent else { break };
            if !seen.insert(parent) {
                return Err(ProjectError::Cycle { branch: current });
            }
            if parent > current {
                // Parents are allocated before their children, so a newer parent means a doctored
                // file even when the walk happens to terminate.
                return Err(ProjectError::NotMonotonic {
                    detail: format!("{} claims the newer parent {parent}", branch.id),
                });
            }
            chain.push(parent);
            current = parent;
        }
        chain.reverse();
        if chain.first() != Some(&BranchId::SOURCE) {
            return Err(ProjectError::BadSource {
                detail: format!("{id} does not descend from branch-0"),
            });
        }
        Ok(chain)
    }

    /// Fork `parent` at a frame boundary. The child starts unverified and covers `frame + 1` frames,
    /// because it has to reach the fork before it can diverge.
    ///
    /// # Errors
    /// [`ProjectError::UnknownBranch`], [`ProjectError::FrameBeyondBranch`],
    /// [`ProjectError::TooManyBranches`] or [`ProjectError::FramesExceeded`]. The project is
    /// unchanged when this returns `Err`.
    pub fn fork(
        &mut self,
        parent: BranchId,
        frame: u64,
        title: Option<String>,
        seats: Vec<SeatSetting>,
    ) -> Result<BranchId, ProjectError> {
        let parent_branch = self
            .branch(parent)
            .ok_or(ProjectError::UnknownBranch { branch: parent })?;
        if frame >= parent_branch.frames {
            return Err(ProjectError::FrameBeyondBranch {
                branch: parent,
                limit: parent_branch.frames,
                wanted: frame,
            });
        }
        if self.branches.len() >= MAX_BRANCHES_PER_PROJECT as usize {
            return Err(ProjectError::TooManyBranches {
                max: MAX_BRANCHES_PER_PROJECT,
            });
        }
        let frames = frame + 1;
        let total = self.total_frames().saturating_add(frames);
        if total > MAX_TOTAL_FRAMES {
            return Err(ProjectError::FramesExceeded {
                total,
                max: MAX_TOTAL_FRAMES,
            });
        }
        if self.next_branch >= MAX_BRANCHES_PER_PROJECT {
            return Err(ProjectError::TooManyBranches {
                max: MAX_BRANCHES_PER_PROJECT,
            });
        }
        let id = BranchId::new(self.next_branch);
        self.branches.push(Branch {
            id,
            parent: Some(parent),
            origin: Origin::Fork { frame },
            title,
            seats,
            answers: Vec::new(),
            frames,
            verified: false,
            ui: None,
        });
        self.next_branch = self.next_branch.saturating_add(1);
        self.restyle();
        Ok(id)
    }

    /// Every decision that must be forced to reproduce `id`: the answers on the chain, each
    /// ancestor cut at the frame where its child diverged, ordered by `(frame, ask)`.
    ///
    /// A branch that has not been played yet is reproduced entirely by its ancestors' answers; a
    /// branch forked at frame 5 inherits what happened before frame 5 and answers the rest itself.
    ///
    /// # Errors
    /// Propagates [`path_to`](Self::path_to) refusals.
    pub fn prefix(&self, id: BranchId) -> Result<Vec<ReplayRecord>, ProjectError> {
        let chain = self.path_to(id)?;
        let mut answer: Vec<ReplayRecord> = Vec::new();
        for (position, branch_id) in chain.iter().enumerate() {
            let branch = self
                .branch(*branch_id)
                .ok_or(ProjectError::UnknownBranch { branch: *branch_id })?;
            // The last entry is the branch itself: keep all of its answers. Ancestors are cut where
            // the next branch on the chain diverged from them.
            //
            // The cut is inclusive, and that is the whole of the arithmetic here. A decision is
            // stamped with the number of frames that existed when the engine asked it, so the
            // answers stamped `F` are the ones the step that *produces* frame `F` consumes. A fork
            // at frame `F` is played from the position frame `F` shows, so reaching that position
            // means replaying everything up to and including `F`; cutting at `< F` left the last
            // step of the prefix unanswered, and a manual seat then sat parked in the middle of a
            // rebuild waiting to be asked the question it had already answered once.
            let cut = chain
                .get(position + 1)
                .and_then(|child| self.branch(*child))
                .and_then(Branch::fork_frame);
            answer.extend(
                branch
                    .answers
                    .iter()
                    .filter(|record| cut.is_none_or(|frame| record.frame <= frame))
                    .cloned(),
            );
        }
        answer.sort_by_key(|record| (record.frame, record.ask));
        Ok(answer)
    }

    /// The prefix as a script for [`crate::rebuild`], with the branch's own ids rewritten to the
    /// branch being rebuilt.
    ///
    /// # Errors
    /// Propagates [`prefix`](Self::prefix) refusals.
    pub fn replay_script(&self, id: BranchId) -> Result<ReplayScript, ProjectError> {
        Ok(ReplayScript::new(self.prefix(id)?))
    }

    /// Frames all branches replay between them, charged against [`MAX_TOTAL_FRAMES`].
    #[must_use]
    pub fn total_frames(&self) -> u64 {
        self.branches
            .iter()
            .fold(0_u64, |total, branch| total.saturating_add(branch.frames))
    }

    /// Mark a branch reproduced. Call this only with the result of an actual rebuild; that is why it
    /// is not a public field.
    ///
    /// # Errors
    /// [`ProjectError::UnknownBranch`] when the project does not contain `id`.
    pub fn mark_verified(&mut self, id: BranchId) -> Result<(), ProjectError> {
        let branch = self
            .branches
            .iter_mut()
            .find(|branch| branch.id == id)
            .ok_or(ProjectError::UnknownBranch { branch: id })?;
        branch.verified = true;
        self.restyle();
        Ok(())
    }

    /// Extend a branch with answers made while it was played, and the frames it now covers.
    ///
    /// # Errors
    /// [`ProjectError::UnknownBranch`] or [`ProjectError::FramesExceeded`] when growing the branch
    /// would take the project past its frame budget.
    pub fn extend(
        &mut self,
        id: BranchId,
        answers: Vec<ReplayRecord>,
        frames: u64,
    ) -> Result<(), ProjectError> {
        let index = self
            .branches
            .iter()
            .position(|branch| branch.id == id)
            .ok_or(ProjectError::UnknownBranch { branch: id })?;
        let total = self
            .total_frames()
            .saturating_sub(self.branches[index].frames)
            .saturating_add(frames);
        if total > MAX_TOTAL_FRAMES {
            return Err(ProjectError::FramesExceeded {
                total,
                max: MAX_TOTAL_FRAMES,
            });
        }
        let branch = &mut self.branches[index];
        branch.answers = answers;
        branch
            .answers
            .sort_by_key(|record| (record.frame, record.ask));
        branch.frames = frames;
        self.restyle();
        Ok(())
    }

    /// Record how many frames a branch now covers, without touching its answers.
    ///
    /// A live table grows a frame at a time, and [`fork`](Self::fork) refuses a frame past the end of
    /// the parent - so a branch whose count is never updated can never be forked from. The window
    /// therefore reports the count as frames arrive; the answers are folded in separately, because
    /// they are only known once the gate has settled them.
    ///
    /// # Errors
    /// [`ProjectError::UnknownBranch`] or [`ProjectError::FramesExceeded`].
    pub fn note_frames(&mut self, id: BranchId, frames: u64) -> Result<(), ProjectError> {
        let index = self
            .branches
            .iter()
            .position(|branch| branch.id == id)
            .ok_or(ProjectError::UnknownBranch { branch: id })?;
        if self.branches[index].frames == frames {
            return Ok(());
        }
        let total = self
            .total_frames()
            .saturating_sub(self.branches[index].frames)
            .saturating_add(frames);
        if total > MAX_TOTAL_FRAMES {
            return Err(ProjectError::FramesExceeded {
                total,
                max: MAX_TOTAL_FRAMES,
            });
        }
        self.branches[index].frames = frames;
        self.restyle();
        Ok(())
    }

    /// Attach a UI selection to a branch. Presentation only: it is preserved and never interpreted.
    ///
    /// # Errors
    /// [`ProjectError::UnknownBranch`].
    pub fn set_ui(&mut self, id: BranchId, ui: Option<Value>) -> Result<(), ProjectError> {
        let branch = self
            .branches
            .iter_mut()
            .find(|branch| branch.id == id)
            .ok_or(ProjectError::UnknownBranch { branch: id })?;
        branch.ui = ui;
        self.restyle();
        Ok(())
    }

    /// Every structural rule, plus the byte bound. Load and save both run this before touching disk.
    ///
    /// # Errors
    /// Any structural variant of [`ProjectError`].
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.schema != PROJECT_SCHEMA || self.version != PROJECT_VERSION {
            return Err(ProjectError::Unsupported {
                schema: self.schema.clone(),
                version: self.version,
            });
        }
        if !self.inputs.temperature.is_finite() || self.inputs.temperature <= 0.0 {
            return Err(ProjectError::Malformed {
                detail: "sampling temperature is not a positive finite number".to_owned(),
            });
        }
        if self.branches.is_empty() {
            return Err(ProjectError::BadSource {
                detail: "no branches at all".to_owned(),
            });
        }
        let roots: Vec<BranchId> = self
            .branches
            .iter()
            .filter(|branch| branch.parent.is_none())
            .map(|branch| branch.id)
            .collect();
        if roots != [BranchId::SOURCE] {
            return Err(ProjectError::BadSource {
                detail: format!("parentless branches: {roots:?}"),
            });
        }
        let mut seen: std::collections::BTreeSet<BranchId> = std::collections::BTreeSet::new();
        for branch in &self.branches {
            if !seen.insert(branch.id) {
                return Err(ProjectError::NotMonotonic {
                    detail: format!("{} appears twice", branch.id),
                });
            }
            if branch.id.index() >= self.next_branch {
                return Err(ProjectError::NotMonotonic {
                    detail: format!("{} is not below the allocator's next id", branch.id),
                });
            }
            if let Origin::Fork { frame } = branch.origin {
                let parent = branch.parent.ok_or(ProjectError::BadSource {
                    detail: format!("{} forks without a parent", branch.id),
                })?;
                if parent > branch.id {
                    return Err(ProjectError::NotMonotonic {
                        detail: format!("{} claims the newer parent {parent}", branch.id),
                    });
                }
                if frame >= branch.frames {
                    return Err(ProjectError::FrameBeyondBranch {
                        branch: branch.id,
                        limit: branch.frames,
                        wanted: frame,
                    });
                }
            } else if branch.parent.is_some() {
                return Err(ProjectError::BadSource {
                    detail: format!("{} is imported but has a parent", branch.id),
                });
            }
            for record in &branch.answers {
                if record.offered.is_empty() {
                    return Err(ProjectError::Malformed {
                        detail: format!("{} holds a record with no offered options", branch.id),
                    });
                }
                if !record.offered.contains(&record.chosen) {
                    return Err(ProjectError::Malformed {
                        detail: format!(
                            "{} records a choice of {:?}, which was not offered",
                            branch.id, record.chosen
                        ),
                    });
                }
            }
        }
        for branch in &self.branches {
            if let Some(parent) = branch.parent
                && self.branch(parent).is_none()
            {
                return Err(ProjectError::MissingParent {
                    branch: branch.id,
                    parent,
                });
            }
            // Walking every branch catches a cycle even when the cycle is disconnected from the
            // source, which a single top-down traversal would silently skip.
            self.path_to(branch.id)?;
        }
        let total = self.total_frames();
        if total > MAX_TOTAL_FRAMES {
            return Err(ProjectError::FramesExceeded {
                total,
                max: MAX_TOTAL_FRAMES,
            });
        }
        if self.branches.len() > MAX_BRANCHES_PER_PROJECT as usize {
            return Err(ProjectError::TooManyBranches {
                max: MAX_BRANCHES_PER_PROJECT,
            });
        }
        Ok(())
    }

    /// The payload bytes the file would hold, which is what the byte bound charges.
    ///
    /// # Errors
    /// [`ProjectError::Malformed`] if the project cannot be serialized at all.
    pub fn payload_bytes(&self) -> Result<Vec<u8>, ProjectError> {
        serde_json::to_vec(self).map_err(|error| ProjectError::Malformed {
            detail: error.to_string(),
        })
    }

    /// Recompute the checksum field after a mutation. Private so every mutation goes through it.
    fn restyle(&mut self) {
        self.checksum = checksum_of(self);
    }

    /// The checksum the payload should carry.
    #[must_use]
    pub fn expected_checksum(&self) -> String {
        checksum_of(self)
    }
}

/// The content corpus this build plays with, as R01 records it.
#[must_use]
pub fn embedded_content() -> String {
    ti4_content::embedded_digest().corpus
}

/// Refuse a source whose content digest is known and disagrees with this build. Replay reads the
/// same content the engine projected, so a different corpus is a different game and no amount of
/// determinism makes the two interchangeable.
fn require_content(recorded: Option<&str>) -> Result<(), ProjectError> {
    let Some(recorded) = recorded else {
        // A session written before the digest existed cannot disagree with it.
        return Ok(());
    };
    let found = embedded_content();
    if found == recorded {
        Ok(())
    } else {
        Err(ProjectError::ProvenanceMismatch {
            what: "content digest",
            expected: recorded.to_owned(),
            found,
        })
    }
}

fn checksum_of(project: &ReplayerProject) -> String {
    let mut blanked = project.clone();
    blanked.checksum = String::new();
    let payload = serde_json::to_vec(&blanked).unwrap_or_default();
    hex_digest(&payload)
}

/// Turn an R01 recording's decisions into the answers branch-0 stands on.
///
/// A session is drawn from the frames it recorded, but a project is a recipe, and forking at frame N
/// has to *force* the decisions that produced frames zero through N. Without them a rebuild is a fresh
/// policy play that happens to look like the game on the screen, which is not the promise R02 makes:
/// `play_check` would green-light a branch whose frames are nobody's record. R01 stores everything the
/// record needs - actor, prompt, the offered options in engine order with their kinds, the chosen id,
/// and the typed context - so these answers come out of the file rather than being invented here.
///
/// A decision with no recorded answer is skipped. It was a decision the recorder never saw settled, and
/// forcing "nothing" onto a seat would be worse than leaving that one ask to the seat's mode.
///
/// # Errors
/// [`ProjectError::Malformed`] when a recording claims a decision was settled on an option it never
/// offered. That is a corrupt or doctored file, and a prefix built from it would look like a legal game
/// while replaying one that could not have been played.
pub fn answers_from_session(
    session: &ti4_review::ReviewSession,
) -> Result<Vec<ReplayRecord>, ProjectError> {
    let mut records = Vec::new();
    for (frame, state) in session.frames.iter().enumerate() {
        let frame = u64::try_from(frame).unwrap_or(u64::MAX);
        for (ask, decision) in state.decisions.iter().enumerate() {
            let Some(chosen) = decision.chosen.clone() else {
                continue;
            };
            let actor = ti4_model::id::PlayerId::new(&decision.player);
            let offered: Vec<OfferedOption> = decision
                .options
                .iter()
                .map(|option| OfferedOption {
                    id: option.id.clone(),
                    kind: option.kind.clone(),
                    label: option.label.clone(),
                    payload: option.payload.clone(),
                    preview: option.preview.clone(),
                    score: option.score,
                    probability: option.probability,
                })
                .collect();
            if !offered.iter().any(|option| option.id == chosen) {
                return Err(ProjectError::Malformed {
                    detail: format!(
                        "frame {frame} records an answer of {chosen} to {:?}, which was not offered",
                        decision.prompt
                    ),
                });
            }
            let fingerprint = ChoiceFingerprint::compute(
                &actor,
                &decision.prompt,
                &offered,
                decision.context.as_ref(),
            );
            records.push(ReplayRecord {
                branch: BranchId::SOURCE,
                frame,
                ask: u32::try_from(ask).unwrap_or(u32::MAX),
                actor,
                faction: if decision.faction.is_empty() {
                    None
                } else {
                    Some(ti4_model::id::FactionId::new(&decision.faction))
                },
                prompt: decision.prompt.clone(),
                offered: offered.iter().map(|option| option.id.clone()).collect(),
                chosen,
                context: decision.context.clone(),
                fingerprint,
                provenance: Provenance::Policy,
            });
        }
    }
    Ok(records)
}
