# iOS app performance

Repos with 1000+ changed files and chats of 5000+ items are the normal case. Two tools measure them; both run on the Mac, in release.

## Re-running

```sh
export PATH=$HOME/.swiftly/bin:$PATH
cd packages/ios/Packages/AnywhereKit

# Model layer: decode, fold, rows, diff, highlight, search.
swift build -c release --scratch-path /tmp/ak-bench --product anywhere-bench
/tmp/ak-bench/release/anywhere-bench

# Screens: the real SwiftUI views over fixture stores, offscreen. The define adds body counts.
swift build -c release --scratch-path /tmp/ak-bench-ui -Xswiftc -DPERF_COUNTERS --product anywhere-bench
/tmp/ak-bench-ui/release/anywhere-bench ui                 # every scenario, 3 runs each, medians
/tmp/ak-bench-ui/release/anywhere-bench ui chat-5000 --png /tmp/ui   # one run, with PNGs to look at
```

Give `-DPERF_COUNTERS` its own scratch path: changing the flags rebuilds everything. Without it the counters compile to nothing and the body columns read `-`. `UIBENCH_ITEMS` (chat size) and `UIBENCH_FRAMES` (stream frames, for sampling with `sample <pid>`) override the chat fixture; `UIBENCH_PAGES` the scroll pages. `anywhere-bench ui chat-send --png <dir>` sends a prompt at the end of the long chat; its `-updated.png` shows the prompt held a quarter down.

## On iOS, frame by frame

`make -C packages/ios bench-ios` (after `make gen`; `scripts/ios-bench.sh [scenario…]`, `SIM=<udid>` for a simulator not booted) builds the app in Release with `BENCH` and launches it with `-bench <scenario>`: the same `BenchScenes` screens as `anywhere-bench ui`, in the real app. After 1.5 s for off-main work, a `CADisplayLink` applies one update per display frame (600 stream frames), then moves the main scroll view 30 pt per frame (240 frames). Each frame's cost is the main thread's CPU time since the last tick; a hitch is a tick more than 1.5 frames after the one before. `ending … above the end` checks the chat left its end when scrolled up. Debug builds take `-bench` too, for `sample <pid>`.

iPhone 17 Pro simulator, iOS 27.0, 60 Hz (a 16.7 ms frame), on an M-series Mac. ms of main-thread CPU per frame, p50 / p95 / max.

| Scenario | Load ms | Stream | Stream hitches | Scroll | Scroll hitches | Memory MB |
|---|---:|---:|---:|---:|---:|---:|
| chat-5000 | ~750 | 6.1 / 7.4 / 19 | 0–1 / 600 | 3.8 / 12 / 26 | 1–5 / 241 | 37 |
| changes-1500 | ~420 | 7.0 / 30 / 30 | 0–1 / 20 | 3.7 / 6.9 / 12 | 0 / 241 | 38 |
| diff-20k | ~290 | – | – | 1.2 / 3.1 / 12 | 0–1 / 241 | 29 |
| file-10k | ~285 | – | – | 1.1 / 3.5 / 6.8 | 0 / 241 | 26 |
| markdown-258k | ~505 | – | – | 1.5 / 12.7 / 26 | 1–10 / 241 | 40 |

Hitch counts vary between runs of the same build; run a scenario three times before reading a change into them.

- **Markdown and chat scroll hitches are text selection.** `.textSelection(.enabled)` gives each `Text` a `UITextInteraction` and its gesture recognizers as it comes on screen, about a third of the main thread while markdown scrolls. Without it in markdown, scroll falls to 0–3 hitches, max ~19 ms; one modifier over the whole document is worse (load 2.9 s). Keeping per-text selection is a product choice; the iOS alternative is a long-press "Select text" sheet.
- **A table measured each cell twice**; a cell at its natural width or wider now keeps the height it measured then.
- **A Live Activity start blocked the main thread ~0.7 s** in ActivityKit's synchronous XPC; it now runs off it.

## How `anywhere-bench ui` measures

Each scenario runs in its own process (so memory is its own) and hosts the public entry view (`ChatView`, `GitView`, `FilesView`) in an `NSHostingView` inside an offscreen, ordered-in window at phone size (393×852; the split diff at 1024×768).

- **Load**: from making the hosting view to the end of 4 settle passes. A pass is one run-loop turn (main-actor tasks, preference and state changes), `layoutSubtreeIfNeeded`, `displayIfNeeded` and a `CATransaction` flush. Work the screen sends off the main actor (highlighting, a long markdown parse, a file's line split) lands after this, so the file and markdown loads time their first frame, not their coloured one.
- **Update**: one live message applied through the store, then 3 settle passes. Chat: 60 `agent.stream` frames growing the last reply by a word, a new paragraph every 20th. Changes: 20 `git.status` replies with a new rev and one file's counts changed.
- **Scroll**: 12 pages, each moving the main scroll view's clip by its height, then 3 settle passes. The chat is first dragged with a mouse event so it stops following its tail, as a finger would.
- **Times are main-thread CPU time** (`CLOCK_THREAD_CPUTIME_ID`), so other work on the machine doesn't count. They are still noisy by ±20% between runs on a loaded Mac; compare runs made side by side.
- **Memory** is `phys_footprint` after load, fixtures included.
- **Bodies** are `BodyCount.hit` calls in the measured views' `body`, per update or per page of scroll.
- **Following**: chat-5000 also checks the scroll view's gap to its end after load and after the stream, printing `FOLLOW|…|ok`; over 2 pt either time prints `fail` and exits 1.

The Mac's SwiftUI is not the phone's: `List` is an `NSOutlineView` here and a `UICollectionView` on iOS, and a phone CPU is slower. Read the numbers relative to each other, before and after a change.

## Screens (anywhere-bench ui)

Median of 3 runs each, before (`mobile-swift` at `eeab3d4`) and after the `perf/pass` branch, on an M-series Mac. ms of main-thread CPU.

| Scenario | Build | Load ms | Update p50 / p95 ms | Scroll p50 / p95 ms | Memory MB |
|---|---|---:|---:|---:|---:|
| chat-5000 | before | 146.8 | 11.4 / 21.1 | 45.1 / 53.2 | 51 |
| chat-5000 | after | 133.4 | 4.5 / 12.8 | 43.5 / 52.6 | 51 |
| changes-1500 | before | 1119.2 | 299.9 / 408.0 | 71.4 / 124.1 | 105 |
| changes-1500 | after | 114.3 | 3.6 / 30.2 | 44.3 / 79.2 | 33 |
| diff-20k | before | 79.2 | - | 46.4 / 87.2 | 34 |
| diff-20k | after | 48.9 | - | 6.0 / 11.5 | 42 |
| diff-20k-split | before | 94.7 | - | 69.6 / 110.5 | 39 |
| diff-20k-split | after | 50.0 | - | 7.0 / 14.7 | 59 |
| file-10k | before | 26.0 | - | 31.0 / 68.6 | 17 |
| file-10k | after | 26.2 | - | 4.8 / 11.5 | 17 |
| markdown-258k | before | 98.2 | - | 38.3 / 71.0 | 18 |
| markdown-258k | after | 96.7 | - | 37.4 / 70.1 | 18 |

The chat fixture here is the 5000-item `Sample.chatPerf` (8754 rows); the 336 ms load in earlier notes came from an older fixture. chat-5000 follows its end after load and after the stream (`FOLLOW|…|ok`) in both builds.

Body evaluations (after):

| Scenario | To load | Per update | Per page of scroll |
|---|---|---|---|
| chat-5000 | RowView 27, TextRow 7, MarkdownView 7, BlockView 7, PromptBubble 5, TranscriptView 4, ComposerView 2 | RowView 2.0, TextRow 1.9, ComposerView 1.0, MarkdownView 0.2, BlockView 0.2 | RowView 17.6, BlockView 17.3, MarkdownView 11.3, TextRow 11.2, PromptBubble 1.3 |
| changes-1500 | FileRow 15, ChangesScreen 2, CommitComposer 1 | ChangesScreen 1.0, CommitComposer 1.0 | FileRow 22.8 |
| diff-20k | DiffBlock 2 | - | DiffBlock 1.6 |
| diff-20k-split | DiffBlock 2 | - | DiffBlock 1.5 |
| file-10k | CodeTextView 1 (blocks land after load) | - | CodeBlock 1.8 |
| markdown-258k | MarkdownView 2 (blocks land after load) | - | BlockView 16.2 |

Before, a stream frame also re-ran `TranscriptView` (1 per frame), a page of diff built ~48 `DiffLineRow`s (80 side by side) and a page of code ~59 `CodeRow`s; a block now draws 32 rows.

## Model layer (anywhere-bench)

Release, best of 20, same Mac.

| Measurement | Time |
|---|---:|
| decode git.status, 1200 files | 2.53 ms |
| decode git.diff, 6000 rows | 12.05 ms |
| decode agent.timeline, 455 items | 4.16 ms |
| 10 MiB of terminal frames: parse alone | 0.45 ms |
| 10 MiB of terminal frames: parse + SwiftTerm | 522 ms |
| git.status 1500 files: decode + flat rows | 3.35 ms |
| git.status 1500 files: decode + tree rows | 6.06 ms |
| git.diff 20k rows: decode + document | 41.30 ms |
| graph lanes, 5000 commits | 0.82 ms |
| diff row drawn text (CoreText line), per visible row | 23 µs (was 42 µs as an `AttributedString`) |
| index 50k paths for go-to-file | 7.51 ms |
| fuzzy search 50k paths | 1.3–1.9 ms |
| markdown parse, 258 KB | 18.64 ms |
| markdown grow by one chunk, 258 KB doc | 0.03 ms |
| highlight typescript / rust, 513 KB | 4.1 ms |
| chat fold+rows, perf-1200 / perf-5000 | 0.63 / 2.60 ms |
| chat stream frame, perf-5000 | p50 181 µs, p95 198 µs |
| composer tray + agent map scans, perf-5000 | 0.28 ms |

## What this pass changed

- **Changes is a lazy stack of fixed-height rows**, not a `List` (an `NSOutlineView` on the Mac, whose `diffRows` walked all 1500 rows on each refresh). `ChangeLines` caches the flat rows per status revision and fold set (`TreeChanges.revision`), each row is an `Equatable` view with a stable id, and selection lives in `ChangeSelection`, so a refresh redraws nothing that didn't change. Swipe actions (`SwipeRow.swift`, a drag with an axis lock, plus accessibility actions) and multi-select (Edit / Done and a selection bar) are built in, one implementation on both platforms; the context menu stays.
- **Diff and code rows draw in blocks.** A row's text is laid out once into a CoreText line (`DrawnText`, cached per row, appearance and size) and 32 rows draw in one `Canvas` (`DrawnRows`), with fills, word tints and line numbers as runs. A canvas is rasterized whole, so a two-way scroll view draws only the window it shows (`ScrollX`). The inline edit diff in the chat uses the same blocks.
- **Chat rows observe their own boxes.** `TranscriptRows` reads the store under observation tracking and puts each change into the ids (when rows come or go) or into the box of the row it changed; `TranscriptView` reads only the ids, so a stream frame redraws the growing row, not the list of thousands.
- **The chat follows its end from its row frames.** The end marker and the room under the rows ride with the last row, so they are built whenever it is and a scroll to the end lands; `laidOut` keeps to the end on each frame change. The jump pill holds its state while the list snaps back to its end, the pill and sticky-card animations sit on their overlays (on the list they carried any scroll landing in the same update), and the sticky card keeps through frames that put no row on screen (frames reported before the lazy stack corrected its estimates).
- **Harness:** `UIBENCH_PAGES`, the `FOLLOW` check, and the `chat-send` scene for the quarter-down pin.

Visuals were checked against the gallery from `mobile-swift`: Changes, diff (unified, split, long), inline edit diff, code and every chat scene render the same. On the Mac the code gutter is a little tighter (the old layout assumed 7.8 pt per digit of the 10 pt mono font), and chat scenes sit up to a point apart from sub-point rounding.

## Earlier pass

- **Release builds of `Stores` crashed the compiler** (Swift 6.2.4, `AccessEnforcementDom` on `GitStore.loadHistory`). The paging loop moved into its own function.
- **A long diff opened mid-document**: its rows sat in an empty scroll view that SwiftUI centred. The scroll view now comes with its rows; pinned by the `git-diff-long` gallery scene.
- **The composer no longer walks the transcript** for the background tray and the agent chip; `ChatStore` keeps `background` and `agents`, worked out on each refold.

## What is left

- **Chat stream frame p95 is ~13 ms (target 8).** Steady frames are 4–6 ms; the p95 comes from the first frame (up to ~25 ms) and the two frames that insert a paragraph row (13–20 ms). An insert changes the lazy stack's child list, and SwiftUI re-estimates every row before the visible ones (`LazyStack.measureEstimates`, ~8750 rows here) and rediffs the ids. Splitting the ids into head and tail `ForEach`es didn't help; grouping rows per turn would, but turns hold rows with state (`FoldRow`, `PromptBubble`, `MarkdownView`) that regrouping would reset. Ruled out since: the estimates (building only the last 200 or 50 rows left inserts at ~15 ms and made scroll worse), the follow `scrollTo` and the per-row frame preferences (removing either changes nothing). Fixed since, without moving the bench: the reveal no longer sits in row items (a fast stream re-rendered every visible paragraph of the message, 27 rows per insert, now 3), and the list no longer observes a row it built in `onAppear`. Then a message's paragraphs went into list rows four at a time (`textRuns`; each still draws as its own row, so the model rows and the desktop's split are untouched): a streaming reply adds a list row every four paragraphs, not every one. Stream p95 fell to ~9–10 ms (p50 5.3); chat scroll p95 rose to ~60 ms, as list rows are taller. What sets p95 now is the reply's first two frames and a paragraph joining a run; both are SwiftUI layout (plain `Text` in place of `MarkdownView` costs the same). Measure on a device with Instruments before going further.
- **Chat scroll is unchanged, ~43 / 53 ms a page**, nearly all text layout of new markdown rows (`BlockView`, `TextRow`).
- **Changes refresh p95 ~30 ms and scroll ~44 / 79 ms a page** on the Mac. Swipe and multi-select are custom now: try them on a device (scroll vs swipe conflicts, VoiceOver actions, Edit mode).
- **Canvas blocks hold bitmaps**: the split diff is 59 MB against 39, bounded by the window drawing only what's on screen.
- **Markdown file scroll is unchanged**, ~37 / 70 ms a page.
- **Numbers above are the Mac's**; the iOS table under "On iOS, frame by frame" is the one to beat on the phone. Check on a device (120 Hz: an 8.3 ms frame) with Instruments.
