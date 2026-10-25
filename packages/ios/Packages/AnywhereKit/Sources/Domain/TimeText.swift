import Foundation

public enum TimeText {
    /// `12s`, `3m 4s`, `1h 2m`, `1d 3h`, as the desktop's `format_elapsed`.
    public static func elapsed(seconds: Int) -> String {
        let s = max(seconds, 0)
        switch s {
        case ..<60: return "\(s)s"
        case ..<3_600: return "\(s / 60)m \(s % 60)s"
        case ..<86_400: return "\(s / 3_600)h \(s % 3_600 / 60)m"
        default: return "\(s / 86_400)d \(s % 86_400 / 3_600)h"
        }
    }

    /// "now", "12m", "3h", "yesterday", "4d", as the desktop's `util::ago`; empty for no time.
    public static func ago(ms: Int, now: Int) -> String {
        guard ms > 0 else { return "" }
        let m = max(now - ms, 0) / 60_000
        switch m {
        case 0: return "now"
        case ..<60: return "\(m)m"
        case ..<(24 * 60): return "\(m / 60)h"
        case ..<(48 * 60): return "yesterday"
        default: return "\(m / (24 * 60))d"
        }
    }

    /// "15:45" or "9:05": 24-hour time with no padded hour, in the reader's time zone.
    public static func clock(ms: Int, timeZone: TimeZone = .current) -> String {
        let (hour, minute) = parts(ms, timeZone)
        return "\(hour):\(minute < 10 ? "0" : "")\(minute)"
    }

    /// "09:41": both padded, as automations show times.
    public static func paddedClock(ms: Int, timeZone: TimeZone = .current) -> String {
        let (hour, minute) = parts(ms, timeZone)
        return String(format: "%02d:%02d", hour, minute)
    }

    /// "14:05" today, else "Fri 14:05".
    public static func when(ms: Int, now: Int, timeZone: TimeZone = .current) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let at = date(ms)
        let time = paddedClock(ms: ms, timeZone: timeZone)
        if calendar.isDate(at, inSameDayAs: date(now)) { return time }
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        return "\(days[calendar.component(.weekday, from: at) - 1]) \(time)"
    }

    private static func date(_ ms: Int) -> Date { Date(timeIntervalSince1970: Double(ms) / 1000) }

    private static func parts(_ ms: Int, _ timeZone: TimeZone) -> (Int, Int) {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let c = calendar.dateComponents([.hour, .minute], from: date(ms))
        return (c.hour ?? 0, c.minute ?? 0)
    }
}
