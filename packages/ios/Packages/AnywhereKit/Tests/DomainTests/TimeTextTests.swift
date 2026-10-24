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
