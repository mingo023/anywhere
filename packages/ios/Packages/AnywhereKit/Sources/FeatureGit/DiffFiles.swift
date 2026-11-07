import DiffModel

/// The files Next and Previous File step through.
enum DiffFiles: Hashable {
    /// A fixed list, as a commit's.
    case list([String])
    /// A section of the tree's changes, read as they load, pages included.
    case changes(ChangeSection)
    /// A compare's files, read as they load.
    case compare(against: String)

    /// The file `by` steps from `path`; nil past either end.
    static func step(from path: String, by: Int, in siblings: [String]) -> String? {
        guard let i = siblings.firstIndex(of: path), siblings.indices.contains(i + by) else { return nil }
        return siblings[i + by]
    }

    /// The file to show once `path` leaves the list: the next, else the one before.
    static func after(removing path: String, in siblings: [String]) -> String? {
        step(from: path, by: 1, in: siblings) ?? step(from: path, by: -1, in: siblings)
    }
}
