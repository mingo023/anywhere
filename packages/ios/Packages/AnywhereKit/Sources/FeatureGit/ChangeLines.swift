import DiffModel
import Observation
import Stores

/// A row of the Changes list and where it sits in its section's card.
struct ChangeLine: Identifiable, Equatable {
    let row: ChangeRow
    /// The card's top row: rounded above.
    let first: Bool
    /// The card's bottom row: rounded below, with no separator.
    let last: Bool
    var id: String { row.id }

    /// Marks each run of rows under a header as one card.
    nonisolated static func lines(_ rows: [ChangeRow]) -> [ChangeLine] {
        rows.indices.map { i in
            let header = { (j: Int) -> Bool in
                guard rows.indices.contains(j) else { return true }
                if case .header = rows[j] { return true }
                return false
            }
            return ChangeLine(row: rows[i], first: !header(i) && header(i - 1), last: !header(i) && header(i + 1))
        }
    }

    /// The paths a selection of row ids picks in one section.
    nonisolated static func paths(_ selection: Set<String>, in section: ChangeSection) -> [String] {
        let prefix = section.rawValue + ":"
        return selection.filter { $0.hasPrefix(prefix) && !$0.hasSuffix("/") }.map { String($0.dropFirst(prefix.count)) }.sorted()
    }
}

/// The list's lines, worked out once per read of the files, view and fold rather than on every redraw.
@MainActor
final class ChangeLines {
    private struct Key: Equatable {
        let revision: Int
        let tree: Bool
        let folded: Set<String>
    }

    private var key: Key?
    private var lines: [ChangeLine] = []

    func lines(_ changes: TreeChanges, tree: Bool, folded: Set<String>) -> [ChangeLine] {
        let key = Key(revision: changes.revision, tree: tree, folded: folded)
        if key != self.key {
            self.key = key
            lines = ChangeLine.lines(ChangeTree.rows(changes.files, tree: tree, folded: folded))
        }
        return lines
    }
}

/// Edit mode's picked rows, read by each row, so picking one redraws the rows on screen and not the list.
@MainActor
@Observable
final class ChangeSelection {
    var selecting = false
    var ids: Set<String> = []

    func toggle(_ id: String) {
        if ids.remove(id) == nil { ids.insert(id) }
    }
}
