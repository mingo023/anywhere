/// Which of a diff's changes the bar has stepped to, as an index into them.
enum ChangeStep {
    /// The change `by` steps from `current`; from none, Next takes the first and Previous the last. Nil past either end.
    static func step(from current: Int?, by: Int, count: Int) -> Int? {
        let next = (current ?? (by > 0 ? -1 : count)) + by
        return (0..<count).contains(next) ? next : nil
    }

    /// The change kept once a reloaded diff has `count` of them: the same one, else the last.
    static func kept(_ current: Int?, count: Int) -> Int? {
        guard let current, count > 0 else { return nil }
        return min(current, count - 1)
    }
}
