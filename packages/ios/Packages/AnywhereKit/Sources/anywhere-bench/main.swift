import ChatModel
import Domain
import Fixtures
import Foundation
import Highlight
import MarkdownModel
import SwiftTerm
import Wire

// Timings of the hot paths at repo scale. Run in release: swift run -c release anywhere-bench
// `anywhere-bench ui` times the screens themselves instead; see UIBench.
if CommandLine.arguments.count > 1, CommandLine.arguments[1] == "ui" {
    exit(MainActor.assumeIsolated { UIBench.main(Array(CommandLine.arguments.dropFirst(2))) })
}

func measure(_ name: String, runs: Int = 20, _ work: () throws -> Void) rethrows {
    var best = Double.infinity
    for _ in 0..<runs {
        let start = DispatchTime.now().uptimeNanoseconds
        try work()
        best = min(best, Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6)
    }
    print(name.padding(toLength: 44, withPad: " ", startingAt: 0) + String(format: "%8.2f ms", best))
}

let status = try ServerFrame(id: "c1", .gitStatus(Sample.gitStatus(files: 1200))).encoded()
let diff = try ServerFrame(id: "c2", .gitDiff(Sample.diff(lines: 6000))).encoded()
let timeline = try ServerFrame(id: "c3", .agentTimeline(Sample.timeline(turns: 40))).encoded()

try measure("decode git.status, 1200 files") { _ = try ServerFrame.decode(status) }
try measure("decode git.diff, 6000 rows") { _ = try ServerFrame.decode(diff) }
try measure("decode agent.timeline, \(Sample.timeline(turns: 40).items.count) items") { _ = try ServerFrame.decode(timeline) }

/// Drops the emulator's replies, as the phone's terminal view does.
final class Quiet: TerminalDelegate {
    func send(source: SwiftTerm.Terminal, data: ArraySlice<UInt8>) {}
}

let log = Sample.terminalLog(bytes: 10 << 20)
let terminalFrames = stride(from: 0, to: log.count, by: 32 << 10).map {
    TerminalFrame(kind: .output, terminalId: "t-bench", payload: log[$0..<min($0 + (32 << 10), log.count)]).encoded
}
let quiet = Quiet()
try measure("10 MiB of terminal frames: parse alone", runs: 5) {
    for raw in terminalFrames { _ = try TerminalFrame(parsing: raw) }
}
try measure("10 MiB of terminal frames: parse + SwiftTerm", runs: 5) {
    let terminal = SwiftTerm.Terminal(delegate: quiet, options: TerminalOptions(cols: 120, rows: 40, scrollback: 1000))
    for raw in terminalFrames { terminal.feed(byteArray: [UInt8](try TerminalFrame(parsing: raw).payload)) }
}
try gitBench()
let paths = Sample.repoPaths(50_000)
measure("index 50k paths for go-to-file", runs: 5) { _ = FuzzyIndex(paths) }
let fileIndex = FuzzyIndex(paths)
for query in ["e", "expl", "stsw", "pocketdwsserverstatus"] {
    measure("fuzzy search 50k paths, \"\(query)\"") { _ = fileIndex.search(query, limit: 50) }
}
// Markdown and highlighting.
let longDoc = Sample.markdownDocument(bytes: 258 * 1024)
measure("markdown parse, \(longDoc.utf8.count / 1024) KB") { _ = Markdown.parse(longDoc) }
let streamed = Markdown.parse(longDoc)
let grown = longDoc + "Streaming a few more words onto the last paragraph"
measure("markdown grow by one chunk, \(longDoc.utf8.count / 1024) KB doc") { _ = Markdown.parse(grown, growing: streamed) }
let typescript = Sample.typescriptSource(bytes: 512 * 1024)
let rust = Sample.rustSource(bytes: 512 * 1024)
measure("highlight typescript, \(typescript.utf8.count / 1024) KB") { _ = Highlighter.spans(typescript, language: "typescript") }
measure("highlight rust, \(rust.utf8.count / 1024) KB") { _ = Highlighter.spans(rust, language: "rust") }
for n in [1200, 5000] {
    let items = Sample.chatPerf(items: n)
    measure("chat fold+rows, perf-\(n)") {
        var s = Session()
        _ = s.update(1, items, working: false, now: 0)
    }
}

var items = Sample.chatPerf(items: 5000)
var session = Session()
_ = session.update(1, items, working: false, now: 0)
let seq = items.count
items.append(TimelineItem(id: "s-u", seq: seq + 1, ts: 0, body: .user(text: "stream")))
items.append(TimelineItem(id: "s-a", seq: seq + 2, ts: 0, body: .assistant(text: "")))
var took: [Double] = []
var text = ""
for frame in 0..<400 {
    text += frame % 20 == 19 ? "end.\n\n" : "word "
    items[items.count - 1].body = .assistant(text: text)
    let start = DispatchTime.now().uptimeNanoseconds
    _ = session.update(2 + frame, items, working: true, now: 0, unchanged: items.count - 1)
    took.append(Double(DispatchTime.now().uptimeNanoseconds - start) / 1e3)
}
took.sort()
print(
    "chat stream frame, perf-5000".padding(toLength: 44, withPad: " ", startingAt: 0)
        + String(format: "p50 %.0f µs, p95 %.0f µs, max %.0f µs", took[took.count / 2], took[took.count * 95 / 100], took[took.count - 1]))
// What the composer's body used to walk on every keystroke; the store now works it out once per refold.
measure("composer tray + agent map scans, perf-5000") {
    _ = inBackground(session.transcript.turns[...]).map(\.call)
    _ = spawned(session.transcript)
}
