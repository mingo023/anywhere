import Foundation
import Testing
@testable import Stores

@MainActor
final class Batches {
    var all: [[Int]] = []
}

@Suite("Coalescer")
struct CoalescerTests {
    @Test("a burst arrives as a few batches holding everything in order")
    func burst() async throws {
        let batches = Batches()
        let coalescer = Coalescer<Int>(interval: .milliseconds(16)) { batch in batches.all.append(batch) }
        for n in 0..<500 { await coalescer.add(n) }
        try await Task.sleep(for: .milliseconds(200))
        let all = await batches.all
        #expect(all.flatMap(\.self) == Array(0..<500))
        #expect(all.count <= 3)
    }

    @Test("items spaced out arrive in their own batches")
    func spaced() async throws {
        let batches = Batches()
        let coalescer = Coalescer<Int>(interval: .milliseconds(5)) { batch in batches.all.append(batch) }
        // Each waits until the one before landed: a busy main actor (the full suite) may take its time to take a batch.
        await coalescer.add(1)
        try await Self.until { await batches.all.count == 1 }
        await coalescer.add(2)
        try await Self.until { await batches.all.count == 2 }
        #expect(await batches.all == [[1], [2]])
    }

    private static func until(_ done: () async -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while await !done(), ContinuousClock.now < deadline { try await Task.sleep(for: .milliseconds(5)) }
    }
}
