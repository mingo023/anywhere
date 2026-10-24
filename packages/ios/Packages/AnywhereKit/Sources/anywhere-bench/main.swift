import Fixtures
import Foundation
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
try gitBench()
