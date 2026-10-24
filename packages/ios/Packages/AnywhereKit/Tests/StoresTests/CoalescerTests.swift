import Testing
@testable import Stores

/// A sleep the test ends by hand, so what lands in a batch doesn't hang on the wall clock.
final class HeldSleep: Sendable {
    let starts: AsyncStream<Void>
    private let started: AsyncStream<Void>.Continuation
    private let wakes: AsyncStream<Void>
    private let woken: AsyncStream<Void>.Continuation

    init() {
        (starts, started) = AsyncStream.makeStream()
        (wakes, woken) = AsyncStream.makeStream()
    }

    func sleep() async {
        started.yield()
        for await _ in wakes { return }
    }

    func wake() { woken.yield() }
}

@Suite("Coalescer")
struct CoalescerTests {
    @Test("a burst arrives as one batch holding everything in order")
    func burst() async {
        let held = HeldSleep()
        let (batches, got) = AsyncStream.makeStream(of: [Int].self)
        let coalescer = Coalescer<Int>(sleep: { _ in await held.sleep() }) { got.yield($0) }
        var starts = held.starts.makeAsyncIterator()
        var next = batches.makeAsyncIterator()
        for n in 0..<500 { await coalescer.add(n) }
        await starts.next()
        held.wake()
        #expect(await next.next() == Array(0..<500))
    }

    @Test("items spaced out arrive in their own batches")
    func spaced() async {
        let held = HeldSleep()
        let (batches, got) = AsyncStream.makeStream(of: [Int].self)
        let coalescer = Coalescer<Int>(sleep: { _ in await held.sleep() }) { got.yield($0) }
        var starts = held.starts.makeAsyncIterator()
        var next = batches.makeAsyncIterator()
        await coalescer.add(1)
        await starts.next()
        held.wake()
        #expect(await next.next() == [1])
        await coalescer.add(2)
        await starts.next()
        held.wake()
        #expect(await next.next() == [2])
    }

    @Test("finishing hands on what is buffered")
    func finish() async {
        let held = HeldSleep()
        let (batches, got) = AsyncStream.makeStream(of: [Int].self)
        let coalescer = Coalescer<Int>(sleep: { _ in await held.sleep() }) { got.yield($0) }
        var starts = held.starts.makeAsyncIterator()
        var next = batches.makeAsyncIterator()
        await coalescer.add(1)
        await starts.next()
        await coalescer.add(2)
        await coalescer.finish()
        held.wake()
        #expect(await next.next() == [1, 2])
    }
}
