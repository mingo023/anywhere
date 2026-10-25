/// Gathers what arrives within `interval` and hands it on as one batch, so a burst of frames is one main-actor transaction.
public actor Coalescer<Element: Sendable> {
    private var buffer: [Element] = []
    private let wake: AsyncStream<Void>.Continuation

    public init(
        interval: Duration = .milliseconds(16),
        sleep: @escaping @Sendable (Duration) async -> Void = { try? await Task.sleep(for: $0) },
        sink: @escaping @MainActor @Sendable ([Element]) -> Void
    ) {
        let (wakes, wake) = AsyncStream.makeStream(of: Void.self, bufferingPolicy: .bufferingNewest(1))
        self.wake = wake
        Task { await self.run(wakes, interval, sleep, sink) }
    }

    public func add(_ element: Element) {
        buffer.append(element)
        if buffer.count == 1 { wake.yield() }
    }

    /// Stops after handing on what's buffered.
    public func finish() { wake.finish() }

    private func run(
        _ wakes: AsyncStream<Void>,
        _ interval: Duration,
        _ sleep: @Sendable (Duration) async -> Void,
        _ sink: @escaping @MainActor @Sendable ([Element]) -> Void
    ) async {
        for await _ in wakes {
            await sleep(interval)
            let batch = buffer
            buffer = []
            if !batch.isEmpty { await sink(batch) }
        }
        if !buffer.isEmpty {
            let batch = buffer
            buffer = []
            await sink(batch)
        }
    }
}
