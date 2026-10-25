/// Body evaluations by view, for `anywhere-bench ui`. Counts only in a build with `-Xswiftc -DPERF_COUNTERS`; otherwise
/// `hit` is empty and inlined away.
@MainActor
public enum BodyCount {
    #if PERF_COUNTERS
        public static var counts: [String: Int] = [:]
        nonisolated public static let enabled = true
    #else
        nonisolated public static let enabled = false
    #endif

    @inline(__always)
    public static func hit(_ view: StaticString) {
        #if PERF_COUNTERS
            counts["\(view)", default: 0] += 1
        #endif
    }

    public static func take() -> [String: Int] {
        #if PERF_COUNTERS
            defer { counts = [:] }
            return counts
        #else
            return [:]
        #endif
    }
}
