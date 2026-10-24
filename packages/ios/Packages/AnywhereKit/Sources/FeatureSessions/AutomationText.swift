import Domain
import Foundation
import Wire

/// How automations read, as the desktop's `automations/logic.rs` words them.
enum AutomationText {
    private static let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    private static let daysPlural = ["Sundays", "Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays"]

    /// "Weekdays at 09:00", "Fridays at 17:00", "Every 2 hours".
    static func when(_ s: Schedule) -> String {
        switch s {
        case .days(let list, let time):
            return "\(daysText(list)) at \(time)"
        case .interval(let m):
            if m > 0 && m % 1440 == 0 { return every(m / 1440, "day") }
            if m > 0 && m % 60 == 0 { return every(m / 60, "hour") }
            return every(m, "minute")
        }
    }

    /// "Weekdays at 09:00 · next 09:00"; just the schedule while paused.
    static func trigger(_ a: Automation, now: Int, timeZone: TimeZone = .current) -> String {
        guard a.enabled, let next = a.nextRunAt, next > 0 else { return when(a.schedule) }
        return "\(when(a.schedule)) · next \(TimeText.when(ms: next, now: now, timeZone: timeZone))"
    }

    /// What a run's row says: what needs a look, or when a good one ran.
    static func last(_ r: Run, now: Int, timeZone: TimeZone = .current) -> String {
        switch r.status {
        case .waiting: "Needs you"
        case .running, .pending: "Working"
        case .succeeded: TimeText.when(ms: r.startedAt, now: now, timeZone: timeZone)
        case .failed: "Failed"
        case .skipped: "Skipped"
        case .cancelled: "Stopped"
        default: ""
        }
    }

    private static func daysText(_ list: [Int]) -> String {
        var sorted: [Int] = []
        for d in list.sorted(by: { ($0 + 6) % 7 < ($1 + 6) % 7 }) where sorted.last != d { sorted.append(d) }
        switch sorted {
        case [1, 2, 3, 4, 5, 6, 0]: return "Every day"
        case [1, 2, 3, 4, 5]: return "Weekdays"
        case [6, 0]: return "Weekends"
        default:
            if sorted.count == 1, days.indices.contains(sorted[0]) { return daysPlural[sorted[0]] }
            return sorted.filter(days.indices.contains).map { days[$0] }.joined(separator: ", ")
        }
    }

    private static func every(_ n: Int, _ unit: String) -> String {
        n == 1 ? "Every \(unit)" : "Every \(n) \(unit)s"
    }
}
