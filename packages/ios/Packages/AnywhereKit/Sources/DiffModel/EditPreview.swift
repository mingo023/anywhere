import Wire

public enum EditPreview {
    /// An agent edit's lines as git's diff lines, each hunk under its header (pocket/chat/diff.rs `lines`). The agent
    /// numbers a deletion by its old line and the rest by their new one; a jump in numbering starts a new hunk.
    public static func lines(_ diff: FileDiff) -> [UnifiedLine] {
        var out: [UnifiedLine] = []
        out.reserveCapacity(diff.lines.count + 2)
        // New minus old line number; the preview starts before the first change, so at zero.
        var shift = 0
        var next: (old: Int, new: Int)?
        for l in diff.lines {
            let n = l.number ?? 0
            let del = l.kind == .del
            let (old, new) = del ? (n, n + shift) : (max(n - shift, 0), n)
            let continues: Bool =
                switch next {
                case nil: false
                case let next?: del ? old == next.old : new == next.new
                }
            if !continues {
                out.append(UnifiedLine(id: out.count, kind: .hunk, text: "@@ -\(old) +\(new) @@"))
            }
            switch l.kind {
            case .del:
                out.append(UnifiedLine(id: out.count, kind: .deleted, old: old, text: l.text))
                shift -= 1
                next = (old + 1, new)
            case .add:
                out.append(UnifiedLine(id: out.count, kind: .added, new: new, text: l.text))
                shift += 1
                next = (old, new + 1)
            default:
                out.append(UnifiedLine(id: out.count, kind: .context, old: old, new: new, text: l.text))
                next = (old + 1, new + 1)
            }
        }
        return out
    }
}
