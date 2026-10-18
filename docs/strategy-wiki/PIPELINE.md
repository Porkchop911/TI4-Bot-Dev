# Local SCPT transcription pipeline

The current discovery snapshot selects the latest publisher-dated **faction guide** for every non-neutral engine faction ID. It uses the public SCPT RSS feed, not a scrape of a search result. Council Keleres variants share one guide, and Firmament/Obsidian share one guide. New guide selection ignores future episodes and refuses tied newest dates. Publication age and source compatibility remain visible; newer advice is not automatically more correct.

The latest snapshot has 30 unique episodes / 33 faction IDs / approximately 65.8 audio hours. Raw audio/transcripts/models stay in ignored `out/strategy-wiki/`; this is an internal local research corpus, not a public redistribution of SCPT episodes. The wiki links publisher sources and pinned existing research in the Feelx234 fork. Manual `notes/` pages are preserved on rebuild. Component relationships are structural engine references; transcript mentions are labeled unverified candidates. Generated pages have a marker and refuse to overwrite manual files.

## Setup

Requires Python 3.12, an internet connection for initial feed/audio/model downloads, and enough disk/RAM. The current workspace already has an isolated environment at `out/scpt-whisper-venv`.

```powershell
# For another checkout, use a Python 3.12 executable:
python -m venv out/scpt-whisper-venv
./out/scpt-whisper-venv/Scripts/python.exe -m pip install -r tools/scpt_whisper.lock.txt
```

The locked CPU runtime is faster-whisper 1.2.1 (Whisper through CTranslate2), with PyAV 18.0.0: PyAV 19 removed an argument required by this faster-whisper release. Runtime versions and the dependency lock hash are recorded with each transcript. Initial model download resolves and saves a model commit plus file hashes. Subsequent runs verify the cached model and use it locally without a Hub lookup. No API key, paid transcription service or summarizer is used. The operator authorized GPU Whisper on October 14; unrelated GPU work remains blocked. The current RTX 3090 has sufficient free VRAM for the small.en pilot. Check available VRAM again before each CUDA batch; the two-instance cap is not a memory guarantee.

For CUDA on this Windows workstation, install the pinned NVIDIA libraries into the same isolated environment (CUDA 12 cuBLAS and cuDNN 9; the system CUDA 13 toolkit is insufficient for this runtime):

```powershell
./out/scpt-whisper-venv/Scripts/python.exe -m pip install -r tools/scpt_whisper.gpu.requirements.txt
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py transcribe --device cuda --pilot --faction hacan --workers 1 --threads 2
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py transcribe --device cuda --workers 2 --threads 2 --model small.en
```

CUDA uses int8_float16, at most two model instances, with NVIDIA dependency versions recorded. Windows workers add the isolated environment's NVIDIA DLL directories to their own PATH. GPU settings create a separate transcription version; CPU checkpoints are retained but are not relabeled as GPU results. CPU remains the default and can be resumed with the original settings.

## Commands

```powershell
# Refresh publisher metadata and rebuild the wiki:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py discover

# A 3-minute quality pilot from a point 10 minutes into the Hacan guide:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py transcribe --pilot --faction hacan --workers 1 --threads 4

# Full local batch, eight independent Whisper instances, two CPU threads each:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py transcribe --workers 8 --threads 2 --model small.en

# Stop at the next chunk boundary, retaining downloads and completed chunks:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py stop

# Resume using the same command. Completed artifacts/chunks are validated and reused.
# Rebuild pages after new transcripts or reviewed draft summaries:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py wiki

# Check a future summary before rebuilding:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py validate-summary docs/strategy-wiki/summaries/hacan.json

# Render every complete transcript as a timestamped readable draft:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py readable

# Render one faction's guide, or a bounded number of episodes:
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py readable --faction hacan
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py readable --limit 3
```

Transcription is CPU `int8`, English, beam 5, VAD, word timestamps, and a TI4 vocabulary prompt. Each worker owns one model. The parent bounds instances to eight and total threads to 16 and checks RAM/disk before starting; it records/releases its compute reservation. Current `small.en` is a CPU-throughput starting point, with quality checks required for specialist names. Changing model creates a separate immutable transcription version, not an overwrite.

The queue is a finite snapshot, not an automation. Rerun `discover` when you want newer episodes. Only one transcription runner can own the lock. A demonstrably dead owner permits stale-lock recovery; live or inaccessible owners block a second run. No unrelated process is stopped. `stop` affects only the recorded run and is honored after the current 10-minute audio chunk completes. Ctrl+C also signals a chunk-boundary stop; full checkpoint retention applies.

## Progress and provenance

Run status: `out/strategy-wiki/runs/*.json`. Per-episode chunk status: `out/strategy-wiki/episodes/<episode-title--stable-id>/transcriptions/<settings-hash>/progress.json`. Completed immutable transcript JSON includes segment/word times, source audio SHA256, model revision/file hashes and decoding settings. Low confidence/silence/compression indicators mark segments for review. Pilots never populate complete-transcript pointers or unlock episode citations.

Source data is never executed. Downloads are bounded and restricted to publisher audio domains and the explicitly named upstream repository. Nonzero exits and job errors remain recorded; a partial or empty transcript is never presented as complete. Completed source records and the graph are rebuilt when the finite batch ends; mid-run `wiki` can safely show completed guides.

This tool creates no model training targets. Summaries and later MLP experiments require review under [the summary contract](SUMMARY_CONTRACT.md).

## Readable drafts for review

`readable` renders each selected episode's complete transcript into `out/strategy-wiki/readable/<episode-id>.md`: a header with provenance and quality counts, then five-minute blocks with one `- \`MM:SS–MM:SS\`` line per segment. Segments marked for review carry `⚠`, and each block lists the words below 0.60 confidence with their time and score. `readable/index.md` and `readable/index.json` record the per-episode counts and the rendered file checksums. The renderer reads only through the validated transcript pointer, so a changed or pilot transcript fails instead of rendering. Output is deterministic: no wall-clock time, sorted by episode ID, atomic writes.

These drafts are an internal review and summarization aid in ignored `out/`, not wiki pages and not a public redistribution. A draft summary cites the transcript record and its timestamps, not the rendering. Mean word confidence for the current `small.en` batch is about 0.90–0.92; low-confidence words are mostly filler plus specialist names (for example `Ian` and `yen` for Yin), so strategy content reads clearly while proper nouns need the contract's uncertainty handling.

Episode folders use the publisher episode title (normalized for Windows, capped at 60 characters) followed by a stable ID, e.g. `360-super-emirates-of-hacan-guide--scpt-360-5e107b23a516`. IDs remain unchanged for citations. Existing folders were moved after a checkpoint stop; migration mappings are retained in `out/strategy-wiki/episode-folder-renames-20261014.json`. Historical run result paths describe the old locations; use that mapping to resolve them.

## Concurrent CPU and GPU queue

```powershell
./out/scpt-whisper-venv/Scripts/python.exe tools/scpt_strategy.py transcribe --device cuda --workers 2 --cpu-workers 6 --threads 2 --model small.en
```

One coordinator assigns each episode once to the next available CPU or GPU pool. Two GPU workers plus six CPU workers, each with two host threads, remain within 16 CPU threads. Each device uses its recorded settings and reuses matching checkpoints. The global lock prevents separate runners from racing; concurrency happens inside the owned batch. Stop is checked before dispatching additional episodes, and active chunks are preserved. This setup trades additional RAM for throughput; memory preflight counts all eight instances.

Each completed episode folder contains `transcript.txt` (continuous paragraphs) and `transcript.md` (one linked timestamp per paragraph). Paragraphs merge ASR segments without rewriting words, using sentence boundaries, a word limit and pauses. Speaker identification is unavailable. The central `readable` command now uses paragraphs too, retaining quality annotations separately. Immutable word/segment JSON and checksums remain the citation source; corrected manual text is never silently overwritten by the paragraph exporter.

## Maintained wiki integration

### Interactive graph view

Open `graph-view.html` in a desktop browser: the graph data is embedded, so it works directly from disk without a server, package installation or CDN. Search by component name or stable ID, select a graph node or connection to recenter, and inspect corpus rules/effects and cited strategy conditions in the detail panel. Faction links, component references and source citations lead back to the wiki. Corpus links and strategy drafts start enabled; unverified ASR mentions start hidden. One/two-step neighborhoods are capped at 90 entries with the truncation stated visibly; isolated entries remain searchable. Pan, zoom, Fit view and Back operate locally. Every `wiki` rebuild regenerates the page from `graph.json` plus the template in `tools/scpt_graph.html`. Viewer-only corpus-reference links are read directly from explicit ID fields and remain separate from inferred strategic advice. Reproducible desktop/phone captures and the shared gallery manifest live in `web/e2e/screenshots/DQ-wiki-graph`.

### Component and rules coverage

The rebuild now indexes 18 component categories: the original technologies, units, abilities, leaders, promissory notes, strategy cards, action cards, public/secret objectives and relics, plus agendas, breakthroughs, exploration cards, attachments, planets, systems, galactic events and rule topics. The current corpus contains 1,326 component/rule records (previously 712).

`components.md` renders corpus card/effect text, timing windows, leader unlocks, separate strategy-card primary/secondary effects, agenda outcomes, unit statistics, planet values and anomaly fields. Zero and false values are preserved; omitted fields are not inferred. Twilight's Fall leader variants and Wild Wild Galaxy alternate effects are labeled separately. Explicit ID references link upgrades, required technology, exploration attachments, system planets and faction ownership where a target exists; unresolved targets remain visible. Faction pages also link home planets, home systems and faction-matched breakthroughs. The graph retains each indexed record under `details`, with the originating content file covered by its checksum map.

`RULES_REFERENCE.md` links the eight local Thunder's Edge rule topics and the publisher's official document downloads. Local rule records contain notes and bot-specific notation as well as rule descriptions; they are corpus material, not independently verified official rulings. They do not constitute a complete base-game/Prophecy of Kings rules reference. Component inclusion does not establish active-setup inclusion or engine implementation. Podcast strategy and ASR backlinks retain their draft provenance.

Readable faction guides now live in `docs/strategy-wiki/summaries/*-strategy-summary.md`, beside their draft JSON. The original extraction files remain archived in ignored `out/`; generated wiki navigation no longer requires them. Shared Keleres and Firmament/Obsidian guides retain variant conditions. Edit a maintained guide to improve prose; rebuilding reads it and does not replace it from the extraction archive.

The three `sources/upstream-*.md` files are byte-identical pinned snapshots. `UPSTREAM.md` records their revision, hashes, scope and general strategy sections. Each faction page combines the guide with exact relevant upstream sections (including the shared TE community appendix), explicit comparisons from `source_comparisons.json`, conditional strategy connections, component links and collapsible structured claim evidence. Upstream engine/bot claims remain historical; their approximate citations are not locally verified transcript spans. Comparisons describe disagreements without automatically selecting a winner or adding training labels.

`tools/scpt_strategy.py wiki` regenerates faction pages, the collection, connection index and component backlinks via `tools/scpt_wiki.py`. Generated pages refuse to overwrite manual files; `notes/`, maintained guides, comparisons and upstream snapshots remain manual inputs. Component backlinks target stable claim anchors and show incoming/outgoing strategy edges with conditions and citations. The build verifies each selected transcript once in a fresh per-build cache; a later build revalidates it. Raw local transcript pointers are still required to validate episode citations on rebuild.
