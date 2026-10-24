import ChatModel
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
