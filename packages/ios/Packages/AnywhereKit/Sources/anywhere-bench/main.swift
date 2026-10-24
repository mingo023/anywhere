import Fixtures
import Foundation
import SwiftTerm
import Wire

// Timings of the hot paths at repo scale. Run in release: swift run -c release anywhere-bench
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
