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

Give `-DPERF_COUNTERS` its own scratch path: changing the flags rebuilds everything. Without it the counters compile to nothing and the body columns read `-`. `UIBENCH_ITEMS` (chat size) and `UIBENCH_FRAMES` (stream frames, for sampling with `sample <pid>`) override the chat fixture.

## How `anywhere-bench ui` measures

Each scenario runs in its own process (so memory is its own) and hosts the public entry view (`ChatView`, `GitView`, `FilesView`) in an `NSHostingView` inside an offscreen, ordered-in window at phone size (393×852; the split diff at 1024×768).

- **Load**: from making the hosting view to the end of 4 settle passes. A pass is one run-loop turn (main-actor tasks, preference and state changes), `layoutSubtreeIfNeeded`, `displayIfNeeded` and a `CATransaction` flush. Work the screen sends off the main actor (highlighting, a long markdown parse, a file's line split) lands after this, so the file and markdown loads time their first frame, not their coloured one.
- **Update**: one live message applied through the store, then 3 settle passes. Chat: 60 `agent.stream` frames growing the last reply by a word, a new paragraph every 20th. Changes: 20 `git.status` replies with a new rev and one file's counts changed.
- **Scroll**: 12 pages, each moving the main scroll view's clip by its height, then 3 settle passes. The chat is first dragged with a mouse event so it stops following its tail, as a finger would.
- **Times are main-thread CPU time** (`CLOCK_THREAD_CPUTIME_ID`), so other work on the machine doesn't count. They are still noisy by ±20% between runs on a loaded Mac; compare runs made side by side.
- **Memory** is `phys_footprint` after load, fixtures included.
- **Bodies** are `BodyCount.hit` calls in the measured views' `body`, per update or per page of scroll.

The Mac's SwiftUI is not the phone's: `List` is an `NSOutlineView` here and a `UICollectionView` on iOS, and a phone CPU is slower. Read the numbers relative to each other, before and after a change.

## Screens (anywhere-bench ui)

Median of 3 runs each, before and after this pass, run interleaved on an M-series Mac. ms of main-thread CPU.

| Scenario | Build | Load ms | Update p50 / p95 ms | Scroll p50 / p95 ms | Memory MB |
|---|---|---:|---:|---:|---:|
| chat-5000 | before | 336.7 | 10.0 / 58.5 | 42.8 / 49.6 | 87 |
| chat-5000 | after | 334.7 | 11.6 / 79.2 | 29.9 / 47.9 | 88 |
| changes-1500 | before | 1150.0 | 358.4 / 406.9 | 72.6 / 125.6 | 104 |
| changes-1500 | after | 1116.5 | 299.9 / 403.6 | 71.6 / 123.3 | 105 |
| diff-20k | before | 80.4 | - | 47.1 / 82.5 | 34 |
| diff-20k | after | 79.0 | - | 46.2 / 86.4 | 34 |
| diff-20k-split | before | 96.6 | - | 69.8 / 111.5 | 39 |
| diff-20k-split | after | 94.9 | - | 69.2 / 112.4 | 39 |
| file-10k | before | 26.0 | - | 30.9 / 67.6 | 17 |
| file-10k | after | 26.2 | - | 30.6 / 66.6 | 17 |
| markdown-258k | before | 97.2 | - | 37.8 / 72.2 | 18 |
| markdown-258k | after | 97.4 | - | 38.1 / 71.8 | 18 |

The chat rows differ by no more than run-to-run noise; its changes cut work on keystrokes, which this doesn't time (see the model table).

Body evaluations (after):

| Scenario | To load | Per update | Per page of scroll |
|---|---|---|---|
| chat-5000 | RowView 310, BlockView 297, TextRow 195, MarkdownView 195, PromptBubble 23, TranscriptView 5, ComposerView 2 | RowView 2.1, TextRow 2.0, TranscriptView 1.2, ComposerView 1.0, MarkdownView 0.6, BlockView 0.6 | RowView 18.0, BlockView 17.5, MarkdownView 11.5, TextRow 11.4, PromptBubble 1.2 |
| changes-1500 | FileRow 30, ChangesScreen 2 | ChangesScreen 1.0, CommitComposer 1.0, FileRow 0.1 | FileRow 29.2 |
| diff-20k | DiffLineRow 41 | - | DiffLineRow 48.1 |
| diff-20k-split | DiffLineRow 69 | - | DiffLineRow 80.2 |
| file-10k | CodeTextView 1 (rows land after load) | - | CodeRow 58.7 |
| markdown-258k | MarkdownView 2 (blocks land after load) | - | BlockView 16.2 |

Every list builds only what is on screen when scrolling (a page is ~15 chat rows, ~29 file rows, ~48 diff rows), and a stream frame redraws ~2 rows.

## Model layer (anywhere-bench)

Release, best of 20, same Mac.

| Measurement | Time |
|---|---:|
| decode git.status, 1200 files | 2.63 ms |
| decode git.diff, 6000 rows | 11.93 ms |
| decode agent.timeline, 455 items | 4.17 ms |
| 10 MiB of terminal frames: parse alone | 0.52 ms |
| 10 MiB of terminal frames: parse + SwiftTerm | 542 ms |
| git.status 1500 files: decode + flat rows | 3.53 ms |
| git.status 1500 files: decode + tree rows | 6.45 ms |
| git.diff 20k rows: decode + document | 42.25 ms |
| graph lanes, 5000 commits | 0.88 ms |
| diff row styled text, per visible row | 42 µs |
| index 50k paths for go-to-file | 8.12 ms |
| fuzzy search 50k paths | 1.4–2.1 ms |
| markdown parse, 258 KB | 19.96 ms |
| markdown grow by one chunk, 258 KB doc | 0.03 ms |
| highlight typescript / rust, 513 KB | 4.2 ms |
| chat fold+rows, perf-1200 / perf-5000 | 0.68 / 2.81 ms |
| chat stream frame, perf-5000 | p50 177 µs, p95 185 µs |
| composer tray + agent map scans, perf-5000 | 0.29 ms |

The last line is what the composer's body used to do on every keystroke and stream frame; it now happens once per refold in `ChatStore.flush`.

## What this pass changed

- **Release builds of `Stores` crashed the compiler** (Swift 6.2.4, `AccessEnforcementDom` on `GitStore.loadHistory`), so no release build that links the stores worked. The paging loop moved into its own function.
- **A long diff opened mid-document.** `DiffView` put its rows inside an empty scroll view that SwiftUI centred, and kept that offset when 20k rows arrived (about 45% down, and ~100 pt right side by side). The scroll view now comes with its rows. Pinned by the `git-diff-long` gallery scene.
- **Changes rows are one view each** (`ChangeRowView`) rather than a `@ViewBuilder` switch with an `EmptyView` case: a status refresh at 1500 files went from 358 to 300 ms here.
- **The composer no longer walks the transcript** for the background tray and the agent chip; `ChatStore` keeps `background` and `agents`, worked out on each refold.

## What is left

- **Changes, 1500 files: ~300 ms per status refresh on macOS.** A sample shows nearly all of it in SwiftUI's `OutlineListCoordinator.diffRows` walking every row; our bodies are ~3 ms of it, `ChangeTree.rows` included. On iOS `List` is a different implementation; measure on a device before restructuring away from `List` (selection, swipe actions and edit mode come with it).
- **The chat redraws its list on every stream frame.** `TranscriptView` reads `store.session`, so each frame re-evaluates it and SwiftUI rediffs 5000 ids; that, and a new paragraph inserting a row (the p95), is most of the 10 ms. Splitting the list so rows observe the session themselves (keeping the ids apart in the store) cut the frame to ~3 ms, but at 2000+ items the transcript then opened blank and stopped following the tail: the scroll pinning in `laidOut` depends on the list being re-evaluated. It was reverted; doing it needs the follow and pin logic driven by something other than the list's redraws.
- **Opening a 5000-item chat builds ~310 rows** while it scrolls to the end, about 20 screens' worth.
- **A page of diff or code costs 30–70 ms** when it is a whole new screen of rows (a fling); each row is an `HStack` of `Text`s with an `AttributedString`. Drawing rows with `Canvas`, as the plan intended for `CodeView`, would cut that.
