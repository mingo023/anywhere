import Domain
import Foundation
import Wire

/// How automations read, as the desktop's `automations/logic.rs` words them.
enum AutomationText {
    private static let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    private static let daysPlural = ["Sundays", "Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays"]
    private static let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

    /// "Weekdays at 09:00", "Fridays at 17:00", "Every 2 hours".
    static func when(_ s: Schedule) -> String {
        switch s {
        case .days(let list, let time):
            return "\(daysText(list)) at \(time)"
        case .interval(let m):
            if m > 0 && m % 1440 == 0 { return every(m / 1440, "day") }
            if m > 0 && m % 60 == 0 { return every(m / 60, "hour") }
            return every(m, "minute")
        case .webhook:
            return "On a webhook"
        }
    }

    /// One automation's row: its schedule and where it runs, its last run, and whether it can be switched or run now.
    struct Line: Hashable, Identifiable {
        let id: String
        let name: String
        let detail: String
        let last: String
        let failed: Bool
        let enabled: Bool
        let reachable: Bool
        /// The session the last run started, to open.
        let agentId: String?
    }

    /// "Every day at 02:00 · anywhere · MacBook Pro", with the last run under it.
    static func line(_ a: Automation, last: Run?, project: String?, mac: String, reachable: Bool, now: Int, timeZone: TimeZone = .current) -> Line {
        Line(
            id: a.id, name: a.name, detail: [when(a.schedule), project, mac].compactMap(\.self).joined(separator: " · "),
            last: lastRun(last, reachable: reachable, now: now, timeZone: timeZone),
            failed: reachable && last?.status == .failed, enabled: a.enabled, reachable: reachable,
            agentId: reachable ? last?.agentId : nil)
    }

    /// "Last run 6h ago · Finished"; a Mac that can't be reached can't say how it is, so "Mac offline · last run Nov 17".
    static func lastRun(_ r: Run?, reachable: Bool, now: Int, timeZone: TimeZone = .current) -> String {
        guard reachable else {
            return ["Mac offline", r.map { "last run \(monthDay(ms: $0.startedAt, timeZone: timeZone))" }].compactMap(\.self).joined(separator: " · ")
        }
        guard let r else { return "Not run yet" }
        let ago = TimeText.ago(ms: r.startedAt, now: now)
        let phrase = ["now": "just now", "yesterday": "yesterday"][ago] ?? "\(ago) ago"
        return ["Last run \(phrase)", status(r.status)].compactMap(\.self).joined(separator: " · ")
    }

    /// The project whose folder holds an automation's, by its name.
    static func project(of folder: String, in projects: [Project]) -> String? {
        projects.first { folder == $0.path || folder.hasPrefix($0.path + "/") }?.name
    }

    private static func status(_ s: RunStatus) -> String? {
        switch s {
        case .waiting: "Needs you"
        case .running, .pending: "Working"
        case .succeeded: "Finished"
        case .failed: "Failed"
        case .skipped: "Skipped"
        case .cancelled: "Stopped"
        default: nil
        }
    }

    private static func monthDay(ms: Int, timeZone: TimeZone) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let day = calendar.dateComponents([.month, .day], from: Date(timeIntervalSince1970: Double(ms) / 1000))
        return "\(months[(day.month ?? 1) - 1]) \(day.day ?? 1)"
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
