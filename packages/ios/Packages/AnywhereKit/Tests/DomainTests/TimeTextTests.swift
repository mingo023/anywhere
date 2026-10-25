import Foundation
import Testing
@testable import Domain

@Suite("Time text")
struct TimeTextTests {
    @Test("elapsed reads seconds, then minutes, then hours, then days")
    func elapsed() {
        #expect(TimeText.elapsed(seconds: -5) == "0s")
        #expect(TimeText.elapsed(seconds: 12) == "12s")
        #expect(TimeText.elapsed(seconds: 184) == "3m 4s")
        #expect(TimeText.elapsed(seconds: 3_720) == "1h 2m")
        #expect(TimeText.elapsed(seconds: 97_200) == "1d 3h")
    }

    @Test("ago reads now, minutes, hours, yesterday, then days, and nothing for no time")
    func ago() {
        let now = 10 * 86_400_000
        #expect(TimeText.ago(ms: 0, now: now) == "")
        #expect(TimeText.ago(ms: now + 5_000, now: now) == "now")
        #expect(TimeText.ago(ms: now - 12 * 60_000, now: now) == "12m")
        #expect(TimeText.ago(ms: now - 3 * 3_600_000, now: now) == "3h")
        #expect(TimeText.ago(ms: now - 30 * 3_600_000, now: now) == "yesterday")
        #expect(TimeText.ago(ms: now - 4 * 86_400_000, now: now) == "4d")
    }

    @Test("timestamps read 24-hour time in the reader's zone")
    func clock() {
        let utc = TimeZone(secondsFromGMT: 0)!
        #expect(TimeText.clock(ms: 1_782_920_700_000, timeZone: utc) == "15:45")
        #expect(TimeText.clock(ms: 1_782_920_700_000, timeZone: TimeZone(secondsFromGMT: -16 * 3600)!) == "23:45")
        #expect(TimeText.clock(ms: 1_782_920_700_000, timeZone: TimeZone(secondsFromGMT: -6 * 3600 - 40 * 60)!) == "9:05")
        #expect(TimeText.paddedClock(ms: 1_782_920_700_000, timeZone: TimeZone(secondsFromGMT: -6 * 3600 - 40 * 60)!) == "09:05")
    }

    @Test("a time today reads as its clock, another day adds the weekday")
    func when() {
        let utc = TimeZone(secondsFromGMT: 0)!
        let wednesday = 1_782_920_700_000 // 2026-07-01 15:45 UTC
        #expect(TimeText.when(ms: wednesday, now: wednesday + 3_600_000, timeZone: utc) == "15:45")
        #expect(TimeText.when(ms: wednesday, now: wednesday + 2 * 86_400_000, timeZone: utc) == "Wed 15:45")
    }
}
