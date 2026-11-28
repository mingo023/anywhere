import Foundation
import Wire

/// One line of the history list: a day's heading, or a commit with where it sits in its day's card.
struct HistoryItem: Identifiable, Equatable {
    enum Kind: Equatable {
        case day(String)
        /// `index` is its place among all the commits loaded; `first` and `last` its place in its day.
        case commit(LogCommit, index: Int, first: Bool, last: Bool)
    }

    let id: String
    let kind: Kind
}

enum HistoryRows {
    /// Commits newest first, under a heading each time the day changes.
    nonisolated static func items(_ commits: [LogCommit], now: Int, calendar: Calendar = .current) -> [HistoryItem] {
        var out: [HistoryItem] = []
        var day: Date?
        for (i, commit) in commits.enumerated() {
            let start = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(commit.at)))
            if start != day {
                day = start
                out.append(HistoryItem(id: "day-\(commit.sha)", kind: .day(GitText.day(at: commit.at, now: now, calendar: calendar))))
            }
            let sameDay = { (j: Int) -> Bool in
                commits.indices.contains(j) && calendar.isDate(Date(timeIntervalSince1970: TimeInterval(commits[j].at)), inSameDayAs: Date(timeIntervalSince1970: TimeInterval(commit.at)))
            }
            out.append(HistoryItem(id: commit.sha, kind: .commit(commit, index: i, first: !sameDay(i - 1), last: !sameDay(i + 1))))
        }
        return out
    }
}
