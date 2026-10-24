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
        await coalescer.add(1)
        try await Task.sleep(for: .milliseconds(100))
        await coalescer.add(2)
        try await Task.sleep(for: .milliseconds(100))
        #expect(await batches.all == [[1], [2]])
    }
}
