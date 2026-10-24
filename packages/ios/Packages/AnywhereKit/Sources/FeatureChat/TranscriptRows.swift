import ChatModel
import Foundation
import Observation
import Stores

/// The row ids the list keys its rows by; they change only when rows come or go, so a stream frame doesn't rediff them.
@MainActor
@Observable
final class RowIds {
    fileprivate(set) var list: [String] = [] {
        didSet { index = Dictionary(list.enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a }) }
    }
    @ObservationIgnored fileprivate(set) var index: [String: Int] = [:]
    /// The model rows each list row shows, by position in `list`.
    @ObservationIgnored fileprivate(set) var spans: [Range<Int>] = []
}

/// The list rows over model rows `rows`: a message's paragraphs in runs of up to `size`, counted from its first, so a
/// streaming reply adds a list row every `size` paragraphs rather than every one, and a run keeps its id as it grows.
func textRuns(_ rows: [Row], size: Int = 4) -> [Range<Int>] {
    var runs: [Range<Int>] = []
    var i = 0
    while i < rows.count {
        var j = i + 1
        if case .text(let turn, let block, _) = rows[i] {
            while j < rows.count, j - i < size, case .text(turn, block, _) = rows[j] { j += 1 }
        }
        runs.append(i..<j)
        i = j
    }
    return runs
}

/// One row's item, observed by that row alone.
@MainActor
@Observable
final class RowBox {
    fileprivate(set) var item: RowItem
    /// The item, read without observing it: `update` runs in the list's `onAppear`, which would tie the list to the row.
    @ObservationIgnored fileprivate var current: RowItem
    /// On screen, so it follows the store; off screen it's brought up to date when it comes back.
    @ObservationIgnored fileprivate var shown = false
    @ObservationIgnored fileprivate var stale = false

    fileprivate init(_ item: RowItem) {
        self.item = item
        current = item
    }
}

/// The transcript's rows kept apart from the list: the store's changes land in the ids when rows come or go, and in the
/// boxes of the rows on screen, so a stream frame redraws the row that grew rather than the list of thousands.
@MainActor
final class TranscriptRows {
    let ids = RowIds()
    private let store: ChatStore
    private var builder: RowBuilder
    private var boxes: [String: RowBox] = [:]

    init(store: ChatStore) {
        self.store = store
        builder = Self.builder(store)
        refresh()
    }

    /// The box for a row, made with its item the first time the list builds the row.
    func box(_ id: String) -> RowBox {
        if let box = boxes[id] { return box }
        let box = RowBox(item(id) ?? RowItem(id: id, gap: 0, rail: .none, content: .hidden))
        boxes[id] = box
        return box
    }

    /// The model row a list row starts at.
    func index(_ id: String) -> Int { ids.index[id].map { ids.spans[$0].lowerBound } ?? -1 }

    func appeared(_ id: String) {
        guard let box = boxes[id] else { return }
        box.shown = true
        if box.stale {
            box.stale = false
            update(box, id)
        }
    }

    func disappeared(_ id: String) { boxes[id]?.shown = false }

    /// Reads what the rows show, and comes back after the next change to any of it.
    private func refresh() {
        builder = withObservationTracking {
            Self.builder(store)
        } onChange: { [weak self] in
            // Called before the change is made: read it once it's in.
            Task { @MainActor [weak self] in self?.refresh() }
        }
        let session = builder.session
        let spans = textRuns(session.rows)
        ids.spans = spans
        let list = spans.map { session.ids[$0.lowerBound] }
        if list != ids.list {
            ids.list = list
            boxes = boxes.filter { ids.index[$0.key] != nil }
        }
        for (id, box) in boxes {
            if box.shown { update(box, id) } else { box.stale = true }
        }
    }

    private func update(_ box: RowBox, _ id: String) {
        guard let item = item(id), item != box.current else { return }
        box.current = item
        box.item = item
    }

    private func item(_ id: String) -> RowItem? {
        ids.index[id].map { builder.item(ids.spans[$0]) }
    }

    private static func builder(_ store: ChatStore) -> RowBuilder {
        RowBuilder(
            session: store.session, provider: store.provider, model: store.summary?.model, ask: store.ask.map { !$0.questions.isEmpty },
            stopping: store.stopping, reveals: store.reveals, settled: store.settled?.row, canSendNow: store.capabilities.sendNow,
            now: Int(Date().timeIntervalSince1970 * 1000))
    }
}
