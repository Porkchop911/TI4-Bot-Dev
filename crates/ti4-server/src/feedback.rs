//! Playtest feedback reports: validation, one JSON file per report, and a terminal lister.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_TEXT_CHARS: usize = 4000;
/// Cap for the ordinary context (everything in `context` except `local`).
pub const MAX_CONTEXT_BYTES: usize = 200 * 1024;
/// Cap for `context.local`, the replay data of an offline game (the save blob dominates: a long
/// game has a few thousand answers and one time/version entry per event, well under 1 MiB).
pub const MAX_LOCAL_BYTES: usize = 2 * 1024 * 1024;
/// Request body cap for the feedback route (the rest of the API keeps 8 KiB): both caps above
/// plus the text and the JSON framing.
pub const MAX_BODY_BYTES: usize = 3 * 1024 * 1024;
/// Most answers an offline save may carry (a whole game is a few thousand).
pub const MAX_LOCAL_ANSWERS: usize = 50_000;
/// Longest `report_id`, and the characters it may use (it becomes a file name).
pub const MAX_REPORT_ID_CHARS: usize = 64;
pub const MIN_REPORT_ID_CHARS: usize = 8;
pub const CATEGORIES: [&str; 4] = ["bug", "confusing", "idea", "other"];

/// Body of `POST /api/feedback`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRequest {
    pub category: String,
    pub text: String,
    /// Auto-collected, viewer-visible context; absent when the sender unticked "Include game info".
    pub context: Option<Value>,
    /// Optional client-generated id (8 to 64 characters of `A-Za-z0-9_-`). A report that is sent
    /// again with the same id (a retry after a lost answer) is stored once; the second send gets
    /// the first one's id back.
    #[serde(default)]
    pub report_id: Option<String>,
}

/// What lands on disk.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredFeedback {
    pub id: String,
    pub received_at: String,
    pub received_unix: u64,
    pub server_commit: String,
    pub category: String,
    pub text: String,
    pub context: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
}

/// Returns the validated request or a human-readable refusal.
pub fn validate(request: &FeedbackRequest) -> Result<(), String> {
    if !CATEGORIES.contains(&request.category.as_str()) {
        return Err(format!("category must be one of {CATEGORIES:?}"));
    }
    let chars = request.text.chars().count();
    if request.text.trim().is_empty() {
        return Err("text must not be empty".to_owned());
    }
    if chars > MAX_TEXT_CHARS {
        return Err(format!("text is limited to {MAX_TEXT_CHARS} characters"));
    }
    if let Some(context) = &request.context {
        if !context.is_object() {
            return Err("context must be an object".to_owned());
        }
        let rest: usize = context
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| key.as_str() != "local")
            .map(|(key, value)| key.len() + value.to_string().len())
            .sum();
        if rest > MAX_CONTEXT_BYTES {
            return Err("context is too large".to_owned());
        }
        if let Some(local) = context.get("local") {
            validate_local(local)?;
        }
    }
    if let Some(id) = &request.report_id {
        let n = id.chars().count();
        if !(MIN_REPORT_ID_CHARS..=MAX_REPORT_ID_CHARS).contains(&n)
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!(
                "report_id must be {MIN_REPORT_ID_CHARS} to {MAX_REPORT_ID_CHARS} characters of A-Z a-z 0-9 _ -"
            ));
        }
    }
    Ok(())
}

/// Checks `context.local`, what an offline game attaches so the position can be replayed:
/// `{ seed, setup, answers, engine: { build, content_digest }, wasm_sha256, save }`, where `save`
/// is the host's save object (`seed`, `setup`, `answers`, `redo`, `never`, `event_times`,
/// `event_versions`, `generation`, `version`, `decisions`, `decisions_hash`; other keys are kept).
fn validate_local(local: &Value) -> Result<(), String> {
    const BAD: &str = "context.local is malformed";
    let bad = |what: &str| Err(format!("{BAD}: {what}"));
    let Some(object) = local.as_object() else {
        return bad("not an object");
    };
    if local.to_string().len() > MAX_LOCAL_BYTES {
        return Err("context.local is too large".to_owned());
    }
    let short = |v: Option<&Value>, max: usize| {
        v.and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty() && s.chars().count() <= max)
    };
    let Some(seed) = object.get("seed").and_then(Value::as_u64) else {
        return bad("seed");
    };
    if !object.get("setup").is_some_and(Value::is_object) {
        return bad("setup");
    }
    let ids = |v: Option<&Value>, name: &str| -> Result<Vec<String>, String> {
        let Some(array) = v.and_then(Value::as_array) else {
            return Err(format!("{BAD}: {name}"));
        };
        if array.len() > MAX_LOCAL_ANSWERS {
            return Err(format!("{BAD}: too many {name}"));
        }
        array
            .iter()
            .map(|a| match a.as_str() {
                Some(s) if s.chars().count() <= 200 => Ok(s.to_owned()),
                _ => Err(format!("{BAD}: {name}")),
            })
            .collect()
    };
    let answers = ids(object.get("answers"), "answers")?;
    let Some(engine) = object.get("engine").and_then(Value::as_object) else {
        return bad("engine");
    };
    if !short(engine.get("build"), 100) || !short(engine.get("content_digest"), 200) {
        return bad("engine.build / engine.content_digest");
    }
    if !short(object.get("wasm_sha256"), 128) {
        return bad("wasm_sha256");
    }
    let Some(save) = object.get("save").and_then(Value::as_object) else {
        return bad("save");
    };
    if save.get("seed").and_then(Value::as_u64) != Some(seed) {
        return bad("save.seed differs from seed");
    }
    if !save.get("setup").is_some_and(Value::is_object) {
        return bad("save.setup");
    }
    if ids(save.get("answers"), "save.answers")? != answers {
        return bad("save.answers differ from answers");
    }
    for key in ["redo", "never"] {
        if let Some(v) = save.get(key) {
            ids(Some(v), &format!("save.{key}"))?;
        }
    }
    for key in ["event_times", "event_versions"] {
        if save.get(key).is_some_and(|v| !v.is_array()) {
            return bad(&format!("save.{key}"));
        }
    }
    for key in ["generation", "version"] {
        if save.get(key).is_some_and(|v| !v.is_u64()) {
            return bad(&format!("save.{key}"));
        }
    }
    if save.get("decisions").and_then(Value::as_u64).is_none() {
        return bad("save.decisions");
    }
    if !short(save.get("decisions_hash"), 64) {
        return bad("save.decisions_hash");
    }
    Ok(())
}

/// What the lister says about a report from an offline game.
#[derive(Debug, PartialEq, Eq)]
pub struct LocalSummary {
    pub seed: u64,
    pub build: String,
    pub decisions: Option<u64>,
    pub answers: usize,
}

/// `Some` for a report that carries `context.local`.
#[must_use]
pub fn local_summary(record: &StoredFeedback) -> Option<LocalSummary> {
    let local = record.context.as_ref()?.get("local")?;
    let clean = |s: &str| {
        s.chars()
            .filter(|c| !c.is_control())
            .take(100)
            .collect::<String>()
    };
    Some(LocalSummary {
        seed: local.get("seed").and_then(Value::as_u64).unwrap_or(0),
        build: local
            .pointer("/engine/build")
            .and_then(Value::as_str)
            .map_or_else(|| "?".to_owned(), clean),
        decisions: local.pointer("/save/decisions").and_then(Value::as_u64),
        answers: local
            .get("answers")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    })
}

/// The save blob (`context.local.save`) of an offline report, for replay.
#[must_use]
pub fn save_blob(record: &StoredFeedback) -> Option<&Value> {
    record.context.as_ref()?.get("local")?.get("save")
}

/// Finds a stored report by its server id (`fb_...`) or by the client's `report_id`.
#[must_use]
pub fn find(data_dir: &Path, id: &str) -> Option<(PathBuf, StoredFeedback)> {
    list_recent(data_dir, usize::MAX)
        .into_iter()
        .find(|(_, r)| r.id == id || r.report_id.as_deref() == Some(id))
}

/// `(year, month, day)` of a unix timestamp in UTC.
#[must_use]
pub fn civil_from_unix(secs: u64) -> (i64, u32, u32) {
    let z = i64::try_from(secs / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Writes the report atomically to `<data_dir>/feedback/YYYY-MM-DD/<id>.json`; returns the id.
pub fn store(
    data_dir: &Path,
    request: FeedbackRequest,
    now_unix: u64,
    commit: &str,
) -> std::io::Result<String> {
    store_once(data_dir, request, now_unix, commit).map(|(id, _)| id)
}

/// Serializes the check-then-write of reports that carry a `report_id`.
static ONCE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Like [`store`], and says whether the report was already there. A request with a `report_id`
/// that was stored before is not written again: the first report's id comes back with `true`.
/// The ids are remembered as `<data_dir>/feedback/.ids/<report_id>` (the report's path under
/// `feedback/`), so the memory survives restarts and a retry after midnight finds its report.
pub fn store_once(
    data_dir: &Path,
    request: FeedbackRequest,
    now_unix: u64,
    commit: &str,
) -> std::io::Result<(String, bool)> {
    let Some(report_id) = request.report_id.clone() else {
        return write_report(data_dir, request, now_unix, commit).map(|id| (id, false));
    };
    let _guard = ONCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = data_dir.join("feedback");
    let marker = root.join(".ids").join(&report_id);
    if let Ok(path) = fs::read_to_string(&marker) {
        let path = path.trim();
        let file = root.join(path);
        // A marker whose report is gone (never finished) is written again.
        if path.ends_with(".json")
            && !path.contains("..")
            && file.is_file()
            && let Some(id) = file.file_stem().and_then(|s| s.to_str())
        {
            return Ok((id.to_owned(), true));
        }
    }
    let id = write_report(data_dir, request, now_unix, commit)?;
    fs::create_dir_all(root.join(".ids"))?;
    let (y, m, d) = civil_from_unix(now_unix);
    let tmp = root.join(".ids").join(format!(".{report_id}.tmp"));
    fs::write(&tmp, format!("{y:04}-{m:02}-{d:02}/{id}.json"))?;
    fs::rename(&tmp, &marker)?;
    Ok((id, false))
}

fn write_report(
    data_dir: &Path,
    request: FeedbackRequest,
    now_unix: u64,
    commit: &str,
) -> std::io::Result<String> {
    let (y, m, d) = civil_from_unix(now_unix);
    let id = format!("fb_{now_unix}_{:08x}", rand::random::<u32>());
    let dir = data_dir
        .join("feedback")
        .join(format!("{y:04}-{m:02}-{d:02}"));
    fs::create_dir_all(&dir)?;
    let secs_of_day = now_unix % 86_400;
    let record = StoredFeedback {
        id: id.clone(),
        received_at: format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            secs_of_day / 3600,
            secs_of_day % 3600 / 60,
            secs_of_day % 60
        ),
        received_unix: now_unix,
        server_commit: commit.to_owned(),
        category: request.category,
        text: request.text,
        context: request.context,
        report_id: request.report_id,
    };
    let tmp = dir.join(format!(".{id}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(&record)?)?;
    fs::rename(&tmp, dir.join(format!("{id}.json")))?;
    Ok(id)
}

/// Most recent reports first.
pub fn list_recent(data_dir: &Path, limit: usize) -> Vec<(PathBuf, StoredFeedback)> {
    let root = data_dir.join("feedback");
    let mut days: Vec<PathBuf> = fs::read_dir(&root)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && !p
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                })
                .collect()
        })
        .unwrap_or_default();
    days.sort();
    days.reverse();
    let mut out = Vec::new();
    for day in days {
        let mut files: Vec<PathBuf> = fs::read_dir(&day)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files.reverse();
        for file in files {
            if let Ok(bytes) = fs::read(&file)
                && let Ok(record) = serde_json::from_slice::<StoredFeedback>(&bytes)
            {
                out.push((file, record));
            }
        }
        if out.len() >= limit {
            break;
        }
    }
    out.sort_by(|a, b| b.1.received_unix.cmp(&a.1.received_unix));
    out.truncate(limit);
    out
}

/// One-line description of who sent the report and who sat at the table, from the optional
/// `context.sender` and `context.players`. `None` for reports that predate names.
#[must_use]
pub fn who_line(record: &StoredFeedback) -> Option<String> {
    let ctx = record.context.as_ref()?;
    let text = |v: &Value, key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .map(|s| {
                s.chars()
                    .filter(|c| !c.is_control())
                    .take(80)
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
    };
    let mut parts = Vec::new();
    if let Some(sender) = ctx.get("sender") {
        let mut name = text(sender, "nickname").or_else(|| text(sender, "typed_name"));
        if let (Some(n), Some(lobby)) = (&mut name, text(sender, "lobby_nickname")) {
            n.push_str(&format!(" (lobby name {lobby})"));
        } else if name.is_none() {
            name = text(sender, "lobby_nickname").map(|l| format!("lobby name {l}"));
        }
        if let Some(n) = name {
            parts.push(format!("from {n}"));
        }
    }
    if let Some(players) = ctx.get("players").and_then(Value::as_array) {
        let table: Vec<String> = players
            .iter()
            .map(|p| {
                let name = text(p, "nickname").unwrap_or_else(|| "?".to_owned());
                let faction = text(p, "faction").unwrap_or_else(|| "?".to_owned());
                let mark = if p.get("is_sender").and_then(Value::as_bool) == Some(true) {
                    "*"
                } else {
                    ""
                };
                format!("{name}{mark} ({faction})")
            })
            .collect();
        if !table.is_empty() {
            parts.push(format!("table: {}", table.join(", ")));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" | "))
}

/// Terminal-readable rendering of one report.
#[must_use]
pub fn render(record: &StoredFeedback, verbose: bool) -> String {
    let mut s = String::new();
    let ctx = record.context.as_ref();
    let get = |key: &str| {
        ctx.and_then(|c| c.get(key))
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
            .unwrap_or_else(|| "-".to_owned())
    };
    let _ = writeln!(
        s,
        "=== {} | {} | {} | client {} | game {} | seat {} | round {} {}",
        record.id,
        record.received_at,
        record.category.to_uppercase(),
        get("build_commit"),
        get("game_id"),
        get("viewer_seat"),
        get("round"),
        get("phase"),
    );
    if let Some(local) = local_summary(record) {
        let _ = writeln!(
            s,
            "    source: offline | seed {} | build {} | decisions {} | answers {} | replay: feedback-save {}",
            local.seed,
            local.build,
            local
                .decisions
                .map_or_else(|| "?".to_owned(), |d| d.to_string()),
            local.answers,
            record.id,
        );
    }
    if let Some(who) = who_line(record) {
        let _ = writeln!(s, "    {who}");
    }
    let _ = writeln!(s, "{}", record.text.trim());
    if verbose && let Some(ctx) = ctx {
        // The save blob is large; `feedback-save` extracts it.
        let mut shown = ctx.clone();
        if let Some(local) = shown.get_mut("local").and_then(Value::as_object_mut) {
            for key in ["save", "answers", "setup"] {
                if let Some(v) = local.get_mut(key) {
                    *v = Value::String(format!("<{} bytes, omitted>", v.to_string().len()));
                }
            }
        }
        let _ = writeln!(
            s,
            "--- context\n{}",
            serde_json::to_string_pretty(&shown).unwrap_or_default()
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1));
        assert_eq!(civil_from_unix(1_760_227_200), (2025, 10, 12));
        assert_eq!(civil_from_unix(1_709_164_800), (2024, 2, 29));
    }

    fn record(context: Option<Value>) -> StoredFeedback {
        StoredFeedback {
            id: "fb_1_ab".to_owned(),
            received_at: "2026-10-13T10:00:00Z".to_owned(),
            received_unix: 1,
            server_commit: "abc".to_owned(),
            category: "bug".to_owned(),
            text: "np gave me a note".to_owned(),
            context,
            report_id: None,
        }
    }

    #[test]
    fn names_are_listed_with_the_table() {
        let r = record(Some(serde_json::json!({
            "sender": { "nickname": "Alex", "lobby_nickname": "Al", "position": 1 },
            "players": [
                { "seat": "p1", "nickname": "Alex", "faction": "sol", "is_sender": true },
                { "seat": "p2", "nickname": "np", "faction": "letnev", "is_sender": false }
            ]
        })));
        let out = render(&r, false);
        assert!(
            out.contains("from Alex (lobby name Al) | table: Alex* (sol), np (letnev)"),
            "{out}"
        );
    }

    #[test]
    fn typed_name_is_used_without_a_nickname_and_control_characters_are_dropped() {
        let r = record(Some(
            serde_json::json!({ "sender": { "typed_name": "Sam\u{1b}[31m" } }),
        ));
        assert_eq!(who_line(&r).as_deref(), Some("from Sam[31m"));
    }

    #[test]
    fn old_reports_without_names_still_load_and_render() {
        let old = r#"{"id":"fb_1_ab","received_at":"x","received_unix":1,"server_commit":"c","category":"bug","text":"t","context":{"game_id":"g","faction":"sol"}}"#;
        let r: StoredFeedback = serde_json::from_str(old).unwrap();
        assert_eq!(who_line(&r), None);
        assert!(!render(&r, false).contains("from "));
        let none: StoredFeedback = serde_json::from_str(r#"{"id":"i","received_at":"x","received_unix":1,"server_commit":"c","category":"bug","text":"t","context":null}"#).unwrap();
        assert_eq!(who_line(&none), None);
    }

    #[test]
    fn stored_report_keeps_names_and_never_a_credential_field() {
        let dir = std::env::temp_dir().join(format!("fbnames_{}", rand::random::<u32>()));
        let request = FeedbackRequest {
            category: "idea".to_owned(),
            text: "hi".to_owned(),
            context: Some(serde_json::json!({ "sender": { "nickname": "Alex" } })),
            report_id: None,
        };
        validate(&request).unwrap();
        let id = store(&dir, request, 1_760_227_200, "abc").unwrap();
        let found = list_recent(&dir, 5);
        assert_eq!(found[0].1.id, id);
        assert_eq!(who_line(&found[0].1).as_deref(), Some("from Alex"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn local(answers: usize) -> Value {
        let ids: Vec<String> = (0..answers).map(|i| format!("opt{i}")).collect();
        serde_json::json!({
            "seed": 7,
            "setup": { "seed": 7, "players": 4 },
            "answers": ids,
            "engine": { "build": "abc123", "content_digest": "d1" },
            "wasm_sha256": "f00d",
            "save": {
                "seed": 7, "setup": { "seed": 7, "players": 4 }, "answers": ids, "redo": [],
                "never": [], "event_times": ["10:00:00"], "event_versions": [1],
                "generation": 0, "version": 5, "decisions": 12, "decisions_hash": "00ff00ff00ff00ff"
            }
        })
    }

    fn offline_request(report_id: Option<&str>) -> FeedbackRequest {
        FeedbackRequest {
            category: "bug".to_owned(),
            text: "stuck after the combat".to_owned(),
            context: Some(
                serde_json::json!({ "sender": { "nickname": "Ada" }, "local": local(3) }),
            ),
            report_id: report_id.map(str::to_owned),
        }
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}_{}", rand::random::<u32>()))
    }

    #[test]
    fn an_offline_report_is_accepted_and_listed_with_its_source() {
        let request = offline_request(Some("rep_0001_abcd"));
        validate(&request).unwrap();
        let dir = temp("fboff");
        let id = store(&dir, request, 1_760_227_200, "srv").unwrap();
        let found = list_recent(&dir, 5);
        let (_, record) = &found[0];
        assert_eq!(record.id, id);
        assert_eq!(record.report_id.as_deref(), Some("rep_0001_abcd"));
        assert_eq!(
            local_summary(record),
            Some(LocalSummary {
                seed: 7,
                build: "abc123".to_owned(),
                decisions: Some(12),
                answers: 3
            })
        );
        let text = render(record, false);
        assert!(
            text.contains("source: offline | seed 7 | build abc123 | decisions 12 | answers 3"),
            "{text}"
        );
        assert!(text.contains("from Ada"), "{text}");
        // Verbose output leaves the save out; the extractor returns it.
        let verbose = render(record, true);
        assert!(!verbose.contains("opt1"), "{verbose}");
        assert_eq!(
            save_blob(record).unwrap()["decisions_hash"],
            "00ff00ff00ff00ff"
        );
        assert_eq!(find(&dir, &id).unwrap().1.id, id);
        assert_eq!(find(&dir, "rep_0001_abcd").unwrap().1.id, id);
        assert!(find(&dir, "nope").is_none());
        // An online report has no source line.
        assert!(!render(&record_without_local(), false).contains("source:"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn record_without_local() -> StoredFeedback {
        record(Some(serde_json::json!({ "game_id": "g" })))
    }

    #[test]
    fn a_retry_with_the_same_report_id_is_stored_once() {
        let dir = temp("fbonce");
        let (first, dup) = store_once(
            &dir,
            offline_request(Some("retry-me-1")),
            1_760_227_200,
            "c",
        )
        .unwrap();
        assert!(!dup);
        // The retry arrives after midnight and with other text: still the first report.
        let mut again = offline_request(Some("retry-me-1"));
        again.text = "changed".to_owned();
        let (second, dup) = store_once(&dir, again, 1_760_227_200 + 90_000, "c").unwrap();
        assert!(dup);
        assert_eq!(second, first);
        assert_eq!(list_recent(&dir, 10).len(), 1);
        assert_eq!(list_recent(&dir, 10)[0].1.text, "stuck after the combat");
        // Another id, and reports without an id, are new reports.
        assert!(
            !store_once(
                &dir,
                offline_request(Some("retry-me-2")),
                1_760_227_300,
                "c"
            )
            .unwrap()
            .1
        );
        assert!(
            !store_once(&dir, offline_request(None), 1_760_227_301, "c")
                .unwrap()
                .1
        );
        assert!(
            !store_once(&dir, offline_request(None), 1_760_227_302, "c")
                .unwrap()
                .1
        );
        assert_eq!(list_recent(&dir, 10).len(), 4);
        // A marker whose report file is gone is written again.
        let (path, _) = find(&dir, &first).unwrap();
        fs::remove_file(path).unwrap();
        let (third, dup) = store_once(
            &dir,
            offline_request(Some("retry-me-1")),
            1_760_227_400,
            "c",
        )
        .unwrap();
        assert!(!dup);
        assert_ne!(third, first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_retries_write_one_report() {
        let dir = temp("fbrace");
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    store_once(
                        &dir,
                        offline_request(Some("same-report-id")),
                        1_760_227_200,
                        "c",
                    )
                    .unwrap()
                    .1
                })
            })
            .collect();
        let duplicates = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|d| *d)
            .count();
        assert_eq!(duplicates, 7);
        assert_eq!(list_recent(&dir, 10).len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_ids_and_local_are_checked() {
        for bad_id in [
            "short",
            "has space 123",
            "../../etc/passwd",
            &"x".repeat(65),
        ] {
            assert!(
                validate(&offline_request(Some(bad_id))).is_err(),
                "{bad_id}"
            );
        }
        let with = |edit: &dyn Fn(&mut Value)| {
            let mut request = offline_request(None);
            edit(request.context.as_mut().unwrap().get_mut("local").unwrap());
            validate(&request)
        };
        assert!(with(&|_| {}).is_ok());
        assert!(with(&|l| *l = Value::String("x".to_owned())).is_err());
        assert!(
            with(&|l| {
                l.as_object_mut().unwrap().remove("save");
            })
            .is_err()
        );
        assert!(
            with(&|l| l["seed"] = 8.into()).is_err(),
            "seed must match the save"
        );
        assert!(with(&|l| l["save"]["answers"] = serde_json::json!(["x"])).is_err());
        assert!(with(&|l| l["engine"]["build"] = "".into()).is_err());
        assert!(with(&|l| l["save"]["decisions_hash"] = 5.into()).is_err());
        assert!(with(&|l| l["answers"] = serde_json::json!([1])).is_err());
    }

    #[test]
    fn the_save_has_its_own_larger_cap_and_does_not_eat_the_context_budget() {
        // A save of about 1 MiB passes although the ordinary context is limited to 200 KiB.
        let mut request = offline_request(None);
        let local = request.context.as_mut().unwrap().get_mut("local").unwrap();
        local["save"]["event_times"] = Value::Array(vec!["10:00:00.000Z".into(); 80_000]);
        assert!(local.to_string().len() > 900_000);
        assert!(validate(&request).is_ok());
        // Above the local cap it is refused.
        let local = request.context.as_mut().unwrap().get_mut("local").unwrap();
        local["save"]["event_times"] = Value::Array(vec!["10:00:00.000Z".into(); 200_000]);
        assert!(validate(&request).unwrap_err().contains("too large"));
        // The rest of the context keeps the old cap, with or without a save next to it.
        let mut request = offline_request(None);
        request.context.as_mut().unwrap()["events"] = "e".repeat(MAX_CONTEXT_BYTES + 1).into();
        assert!(validate(&request).unwrap_err().contains("too large"));
    }
}
