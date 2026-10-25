import ChatModel
import DesignSystem
import MarkdownView
import Stores
import SwiftUI

/// Where a laid-out row sits in the chat's viewport; the list reports only rows it built.
struct RowFrame: Equatable {
    static let top = -1
    static let end = Int.max
    let ix: Int
    let minY: CGFloat
    let maxY: CGFloat

    /// The bottom-most row on screen, not the end marker: what `scrolledUp` compares the next layout against.
    static func seen(_ frames: [RowFrame], viewport: CGFloat) -> RowFrame? {
        frames.last { $0.ix >= 0 && $0.ix != end && $0.maxY > 0 && $0.minY < viewport }
    }

    /// The list scrolled up when a row on screen moved down. Rows growing at the end move none above them, and rows
    /// above re-estimated as they're built shift the offset, not what's on screen.
    static func scrolledUp(from seen: RowFrame?, to frames: [RowFrame]) -> Bool {
        guard let seen, let now = frames.first(where: { $0.ix == seen.ix }) else { return false }
        return now.minY > seen.minY + 1
    }
}

struct RowFrames: PreferenceKey {
    static let defaultValue: [RowFrame] = []
    static func reduce(value: inout [RowFrame], nextValue: () -> [RowFrame]) { value.append(contentsOf: nextValue()) }
}

/// What scrolling reads and writes many times a second, kept out of what redraws the list.
@MainActor
final class ScrollState {
    var proxy: ScrollViewProxy?
    var viewport: CGFloat = 0
    var distance: CGFloat = .infinity
    /// The list follows new rows only while the user left it at the end.
    var following = true
    var dragging = false
    /// A scroll to the end is animating: snapping to the end meanwhile would cut it short.
    var gliding = false
    var pin: Pin?
    /// A row on screen at the last layout, to tell the list scrolling up from rows growing at its end.
    var seen: RowFrame?
    var firstId: String?
}

/// The jump pill and the sticky card, read only by their overlays: a change to them redraws those, not the list.
@MainActor
@Observable
final class Floating {
    var jump = false
    var stuck: Stuck?
}

/// The prompt card over the list's top, and how far the next prompt pushed it.
struct Stuck: Equatable {
    let row: Int
    let shift: Double
}

/// The transcript, bottom-anchored: it follows the tail while at the end, holds a sent prompt a quarter down while
/// its reply grows, offers a jump to the end once far from it, and floats the exchange's prompt over the top.
@MainActor
struct TranscriptView: View {
    let store: ChatStore
    /// A call to show, from a tray or the agent map.
    @Binding var showing: String?
    let openLink: (MarkdownLink) -> Void

    @State private var scroll = ScrollState()
    @State private var spacer: CGFloat = 0
    @State private var floating = Floating()
    @State private var flash: String?
    @State private var halted = false
    @State private var made = Made()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// The rows, made once per store. Not observed: the list reads their ids and each row its own box.
    private final class Made {
        var rows: TranscriptRows?
    }

    var body: some View {
        let _ = BodyCount.hit("TranscriptView")
        let rows = transcriptRows
        let ids = rows.ids.list
        let last = ids.last
        let actions = rowActions
        GeometryReader { outer in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        marker(RowFrame.top)
                        if store.hasOlder {
                            ProgressView().controlSize(.small).frame(maxWidth: .infinity).padding(.vertical, Space.m)
                                .onAppear { store.loadOlder() }
                        }
                        if ids.isEmpty && store.loaded {
                            Text("Send a message to start.").font(ChatFont.small).foregroundStyle(Palette.text3.color)
                                .frame(maxWidth: .infinity).padding(.top, 120)
                        }
                        ForEach(ids, id: \.self) { id in
                            // The end rides with the last row: built whenever it is, so a scroll to the end always lands.
                            VStack(alignment: .leading, spacing: 0) {
                                RowSlot(box: rows.box(id), provider: store.provider, flash: flash == id, actions: actions)
                                    .background(frame(rows.index(id)))
                                    .onAppear { rows.appeared(id) }
                                    .onDisappear { rows.disappeared(id) }
                                if id == last { end }
                            }
                            .id(id)
                        }
                        if ids.isEmpty { end }
                    }
                    .padding(.horizontal, Space.l)
                    .padding(.top, Pin.top)
                    .frame(maxWidth: 760)
                    // Bottom-aligned: a short chat sits on the composer, as the desktop's list does.
                    .frame(maxWidth: .infinity, minHeight: outer.size.height, alignment: .bottom)
                }
                .opensAtEnd()
                .scrollDisabled(halted)
                .coordinateSpace(name: "chat")
                .simultaneousGesture(DragGesture(minimumDistance: 6).onChanged { _ in dragged() }.onEnded { _ in scroll.dragging = false })
                .onPreferenceChange(RowFrames.self) { frames in
                    MainActor.assumeIsolated {
                        scroll.viewport = outer.size.height
                        laidOut(frames)
                    }
                }
                .onAppear {
                    scroll.proxy = proxy
                    scroll.viewport = outer.size.height
                    proxy.scrollTo(endId, anchor: .bottom)
                }
                .onChange(of: outer.size.height) { _, height in scroll.viewport = height }
                .onChange(of: store.sends) { _, _ in
                    scroll.pin = Pin()
                    scroll.following = false
                }
                .onChange(of: store.steers) { _, _ in scroll.pin?.steered() }
                .onChange(of: showing) { _, id in
                    guard let id else { return }
                    showing = nil
                    show(call: id)
                }
                .onChange(of: ids.first) { old, _ in keepTop(old) }
                .overlay(alignment: .bottom) { JumpOverlay(floating: floating) { toEnd() } }
                .overlay(alignment: .top) { StickyOverlay(store: store, floating: floating) { pinToTop($0) } }
            }
        }
    }

    private var rowActions: RowActions {
        RowActions(
            toggle: { [store] key, fallback in
                Haptics.tap()
                store.toggle(key, fallback)
            },
            retry: { [store] id in store.retry(id) },
            sendNow: { [store] in store.sendNow() },
            retryTurn: { [store] turn in store.retryTurn(turn) },
            showCall: { id in show(call: id) },
            openLink: openLink,
            reveal: { [store] in store.reveals[$0] })
    }

    /// What a scroll to the end lands on: the last row, which holds the room under the rows and the end marker. A lazy
    /// stack scrolls only to its own rows, and the last one isn't built once the list is scrolled far from it.
    private var endId: String { transcriptRows.ids.list.last ?? "end" }

    /// Room under the rows for a held prompt and the list's padding, then the end.
    @ViewBuilder
    private var end: some View {
        Color.clear.frame(height: spacer)
        Color.clear.frame(height: Pin.bottom)
        marker(RowFrame.end).id("end")
    }

    private func marker(_ ix: Int) -> some View {
        Color.clear.frame(height: 0).background(frame(ix))
    }

    private func frame(_ ix: Int) -> some View {
        GeometryReader { g in
            let f = g.frame(in: .named("chat"))
            Color.clear.preference(key: RowFrames.self, value: [RowFrame(ix: ix, minY: f.minY, maxY: f.maxY)])
        }
    }

    // MARK: Scrolling

    /// Rows moved: the jump pill, the sticky card and the pin follow, and the list keeps to the tail if it was there.
    private func laidOut(_ frames: [RowFrame]) {
        let viewport = scroll.viewport
        let end = frames.first { $0.ix == RowFrame.end }
        let distance = end.map { $0.minY - viewport } ?? .infinity
        scroll.distance = distance
        if RowFrame.scrolledUp(from: scroll.seen, to: frames) { left() }
        scroll.seen = RowFrame.seen(frames, viewport: viewport)
        if !scroll.dragging && distance <= 2 && scroll.pin == nil { scroll.following = true }
        // Rows grew under a list that keeps to its end: it's back there below, so the pill stays as it was.
        let snaps = scroll.following && scroll.pin == nil && distance > 2 && !scroll.dragging && !scroll.gliding
        let showJump = snaps ? floating.jump : jumpVisibility(wasShown: floating.jump, distance: distance)
        if showJump != floating.jump { floating.jump = showJump }
        updateSticky(frames)
        if scroll.pin != nil { stepPin(frames, end: end) }
        if snaps { scroll.proxy?.scrollTo(endId, anchor: .bottom) }
    }

    private func stepPin(_ frames: [RowFrame], end: RowFrame?) {
        guard var pin = scroll.pin else { return }
        let session = store.session
        guard let row = pin.row(session.rows) else { return }
        let prompt = frames.first { $0.ix == row }
        let top = frames.first { $0.ix == RowFrame.top }
        let viewport = Double(scroll.viewport)
        let above = prompt.map { p in top.map { Double(p.minY - $0.minY) } ?? .infinity } ?? .infinity
        let below = prompt.flatMap { p in end.map { Double($0.minY - spacer - Pin.bottom - p.minY) } }
        let measuring = pin.unmeasured
        guard pin.step(viewport: viewport, above: above, below: below, glide: nil) else {
            if pin.anchored || pin.unmeasured { scroll.following = true }
            scroll.pin = nil
            return
        }
        scroll.pin = pin
        if abs(spacer - pin.spacer) > 0.5 { spacer = pin.spacer }
        guard measuring, !pin.unmeasured, let prompt else { return }
        let id = session.ids[row]
        let held = Pin.held(viewport: viewport, above: above)
        let room = max(viewport - Double(prompt.maxY - prompt.minY), 1)
        let anchor = UnitPoint(x: 0, y: min(max(held / room, 0), 1))
        Task { @MainActor in
            withAnimation(reduceMotion ? nil : .timingCurve(0.33, 1, 0.68, 1, duration: Pin.glide)) {
                scroll.proxy?.scrollTo(id, anchor: anchor)
            }
        }
    }

    /// The user took the list: it follows the tail again only once they bring it back to the end.
    private func dragged() {
        guard !scroll.dragging else { return }
        scroll.dragging = true
        left()
    }

    /// The list moved off the end by the user's doing: it follows the tail again once they bring it back.
    private func left() {
        scroll.gliding = false
        guard scroll.following || scroll.pin != nil else { return }
        scroll.following = false
        scroll.pin?.moved(atEnd: scroll.distance <= 2)
    }

    private func toEnd() {
        scroll.pin = nil
        spacer = 0
        // The pill isn't on the list, so a tap on it leaves a flick's momentum running under the glide: halt it first.
        halted = true
        Task { @MainActor in
            halted = false
            scroll.following = true
            scroll.gliding = !reduceMotion
            withAnimation(reduceMotion ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: Glide.duration)) {
                scroll.proxy?.scrollTo(endId, anchor: .bottom)
            }
            // The animation's own completion fires at once for a scroll, so the glide is timed.
            try? await Task.sleep(for: .seconds(Glide.duration + 0.05))
            scroll.gliding = false
        }
    }

    /// Older rows came in above: the list stays on the row that was first.
    private func keepTop(_ old: String?) {
        guard let old, store.session.ids.contains(old), !scroll.following else { return }
        scroll.proxy?.scrollTo(old, anchor: .top)
    }

    /// Opens what hides the call's phase, then glides to it and flashes it.
    private func show(call id: String) {
        let session = store.session
        guard let (turn, block) = findCall(session.transcript, id), let row = rowOf(session.rows, turn: turn, block: block) else { return }
        store.show(row)
        let rowId = session.ids[row]
        scroll.following = false
        scroll.pin = nil
        withAnimation(reduceMotion ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: Glide.duration)) {
            scroll.proxy?.scrollTo(rowId, anchor: .center)
        }
        flash = nil
        Task { @MainActor in flash = rowId }
    }

    // MARK: Sticky prompt

    private func updateSticky(_ frames: [RowFrame]) {
        let rows = store.session.rows
        let built = frames.filter { $0.ix >= 0 && $0.ix < rows.count }.sorted { $0.ix < $1.ix }
        let viewport = scroll.viewport
        let shown = built.filter { $0.maxY > 0 && $0.minY < viewport }
        // Rows built but none on screen: frames from before the lazy stack corrected its estimates, so keep the card.
        if shown.isEmpty && !built.isEmpty { return }
        guard let top = shown.first, let ix = Sticky.stuck(rows, top: top.ix, offset: Double(max(-top.minY, 0))) else {
            if floating.stuck != nil { floating.stuck = nil }
            return
        }
        let next = Sticky.nextPrompt(rows, top: top.ix).flatMap { n in built.first { $0.ix == n } }
        let shift = Sticky.pushed(bottom: Sticky.marginTop + Sticky.height, next: next.map { Double($0.minY) }).rounded()
        let now = Stuck(row: ix, shift: shift)
        if now != floating.stuck { floating.stuck = now }
    }

    private func pinToTop(_ id: String) {
        scroll.following = false
        withAnimation(reduceMotion ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: Glide.duration)) {
            scroll.proxy?.scrollTo(id, anchor: .top)
        }
    }

    // MARK: Rows

    private var transcriptRows: TranscriptRows {
        if let rows = made.rows { return rows }
        let rows = TranscriptRows(store: store)
        made.rows = rows
        return rows
    }
}

/// A row reading its own box, so a change to one row redraws that row alone.
@MainActor
private struct RowSlot: View {
    let box: RowBox
    let provider: String
    let flash: Bool
    let actions: RowActions

    var body: some View {
        RowView(item: box.item, provider: provider, flash: flash, actions: actions).equatable()
    }
}

/// Each animation on its own overlay: on the list, it would carry a scroll landing in the same update.
@MainActor
private struct JumpOverlay: View {
    let floating: Floating
    let action: () -> Void

    var body: some View {
        ZStack {
            if floating.jump { JumpPill(action: action).padding(.bottom, Space.m) }
        }
        .animation(.timingCurve(0.22, 1, 0.36, 1, duration: 0.18), value: floating.jump)
    }
}

/// The prompt card over the list's top; its own view, so only it reads the session for the prompt.
@MainActor
private struct StickyOverlay: View {
    let store: ChatStore
    let floating: Floating
    let tap: (_ id: String) -> Void

    var body: some View {
        ZStack {
            if let stuck = floating.stuck { card(stuck) }
        }
        .animation(.easeOut(duration: 0.22), value: floating.stuck?.row)
    }

    @ViewBuilder
    private func card(_ stuck: Stuck) -> some View {
        let session = store.session
        if session.rows.indices.contains(stuck.row), case .prompt(let turn) = session.rows[stuck.row],
            let prompt = session.transcript.turns[turn].prompt
        {
            let (text, images) = splitImages(prompt.text)
            let id = session.ids[stuck.row]
            StickyPrompt(text: text, images: images, at: prompt.at, shift: stuck.shift) { tap(id) }
        }
    }
}

extension View {
    /// Opens a long transcript at its end rather than laying it out from the top and chasing the end with `scrollTo`,
    /// which can leave the list on an offset where it builds no rows (under a tall code block, say).
    fileprivate func opensAtEnd() -> some View {
        #if os(iOS)
            defaultScrollAnchor(.bottom, for: .initialOffset)
        #else
            // The macOS 14 SDK has no anchor roles; only the gallery runs this.
            defaultScrollAnchor(.bottom)
        #endif
    }
}
