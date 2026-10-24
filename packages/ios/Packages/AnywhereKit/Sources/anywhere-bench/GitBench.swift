import CodeView
import DiffModel
import Fixtures
import Foundation
import Highlight
import Wire

func gitBench() throws {
    let status = try ServerFrame(id: "g1", .gitStatus(Sample.gitStatus(files: 1500))).encoded()
    try measure("git.status 1500 files: decode + flat rows") {
        guard case .gitStatus(let s) = try ServerFrame.decode(status).message else { return }
        _ = ChangeTree.rows(s.files, tree: false, folded: [])
    }
    try measure("git.status 1500 files: decode + tree rows") {
        guard case .gitStatus(let s) = try ServerFrame.decode(status).message else { return }
        _ = ChangeTree.rows(s.files, tree: true, folded: [])
    }

    let diff = try ServerFrame(id: "g2", .gitDiff(Sample.diff(lines: 20_000))).encoded()
    var doc: DiffDocument?
    try measure("git.diff 20k rows: decode + document") {
        guard case .gitDiff(let d) = try ServerFrame.decode(diff).message else { return }
        doc = DiffDocument(d)
    }

    let log = Sample.log(commits: 5_000)
    measure("graph lanes, 5000 commits") {
        var layout = GraphLayout(tips: log.tips)
        layout.append(log.commits)
    }

    let lines = Array(doc!.lines.prefix(2_000))
    let spans = lines.map { Highlighter.spans($0.text, language: "swift") }
    var total = 0.0
    let runs = 20
    for _ in 0..<runs {
        let start = DispatchTime.now().uptimeNanoseconds
        for (i, line) in lines.enumerated() {
            _ = CodeStyle.attributed(line.text, spans: spans[i], words: line.words, word: .green, dark: false)
        }
        total = min(total == 0 ? .infinity : total, Double(DispatchTime.now().uptimeNanoseconds - start) / 1e3 / Double(lines.count))
    }
    print("diff row styled text, per visible row".padding(toLength: 44, withPad: " ", startingAt: 0) + String(format: "%8.2f µs", total))
}
