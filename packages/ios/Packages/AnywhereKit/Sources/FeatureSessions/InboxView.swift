import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// Every paired Mac's sessions that want a look, in groups: Needs you, with Deny and Approve for a plain ask, Failed and
/// Finished, then what is Working. The Inbox tab's root.
@MainActor
public struct InboxView: View {
    let hosts: [HostModel]
    let open: ((Route) -> Void)?
    @State private var back: Bool
    @State private var workingOpen = false
    /// Sessions whose answer has already failed, for the gallery.
    let failedAnswers: Set<String>

    public init(hosts: [HostModel], open: ((Route) -> Void)? = nil) {
        self.init(hosts: hosts, open: open, back: false, failedAnswers: [])
    }

    init(hosts: [HostModel], open: ((Route) -> Void)?, back: Bool, failedAnswers: Set<String>) {
        self.hosts = hosts
        self.open = open
        _back = State(initialValue: back)
        self.failedAnswers = failedAnswers
    }

    public var body: some View {
        let marks = hosts.flatMap(\.reachabilityMarks).sorted()
        TimelineView(.explicit(marks)) { _ in
            screen(InboxLayout.build(macs(at: .now), back: back), loading: loading)
        }
        .screenBackground()
        .cornerButtons { NewSessionButton(hosts: hosts, open: open) }
        .onChange(of: hosts.map(\.offlineSince)) { was, now in
            if zip(was, now).contains(where: { $1 == nil && Reachability.backOnline(offlineSince: $0, at: .now) }) { back = true }
        }
        .task(id: back) {
            guard back else { return }
            try? await Task.sleep(for: .seconds(2))
            back = false
        }
    }

    private var loading: Bool {
        !hosts.contains { $0.agentsListed } && hosts.contains { [.online, .connecting, .reconnecting].contains($0.reachability(at: .now)) }
    }

    private func screen(_ built: InboxLayout.Built, loading: Bool) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if loading {
                    InboxSkeleton()
                } else {
                    ScreenTitle("Inbox", detail: built.subtitle)
                    if let strip = built.strip {
                        NetStrip(strip.kind, strip.text, action: strip.retry ? ("Retry", { retry(strip.mac) }) : nil)
                            .padding(.horizontal, Inset.margin)
                            .padding(.top, Space.s)
                    }
                    if built.isEmpty {
                        if built.strip == nil { allClear }
                    } else {
                        group("Needs you", built.needsYou)
                        group("Failed", built.failed)
                        group("Finished", built.finished)
                        working(built.working)
                    }
                }
            }
            .padding(.bottom, Space.xl)
        }
    }

    private var allClear: some View {
        VStack(spacing: Space.xs + 2) {
            Text("All clear").font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color)
            Text("Sessions that need you, fail or finish show here.").font(.system(size: 15)).foregroundStyle(Palette.text2.color)
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 48)
        .padding(.top, 150)
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func group(_ title: String, _ entries: [InboxLayout.Entry]) -> some View {
        if !entries.isEmpty {
            GroupHeader(title)
            rows(entries)
        }
    }

    private func rows(_ entries: [InboxLayout.Entry]) -> some View {
        LazyVStack(spacing: 0) { list(entries) }.insetGroup()
    }

    @ViewBuilder
    private func list(_ entries: [InboxLayout.Entry], from first: Int = 0) -> some View {
        ForEach(Array(entries.enumerated()), id: \.element.key) { index, entry in
            if let host = hosts.first(where: { $0.id == entry.mac }), let session = host.sessions.session(entry.id) {
                InboxRow(host: host, session: session, entry: entry, showMac: hosts.count > 1, failed: failedAnswers.contains(entry.id))
                    .rowDivider(first + index > 0)
            }
        }
    }

    @ViewBuilder
    private func working(_ entries: [InboxLayout.Entry]) -> some View {
        if !entries.isEmpty {
            VStack(spacing: 0) {
                Button {
                    Haptics.tap()
                    withAnimation(.snappy(duration: 0.25)) { workingOpen.toggle() }
                } label: {
                    HStack(spacing: 0) {
                        DotSpinner().frame(width: Inset.textColumn)
                        Text("Working").font(.system(size: 17)).foregroundStyle(Palette.text.color).frame(maxWidth: .infinity, alignment: .leading)
                        Text("\(entries.count)").font(.system(size: 17).monospacedDigit()).foregroundStyle(Palette.text2.color)
                        LucideIcon(.chevronRight, size: 17).foregroundStyle(Palette.text3.color).rotationEffect(.degrees(workingOpen ? 90 : 0))
                            .padding(.leading, Space.xs).padding(.trailing, Space.m)
                    }
                    .frame(height: HitTarget.minimum)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityValue(workingOpen ? "Shown" : "Hidden")
                if workingOpen { list(entries, from: 1) }
            }
            .insetGroup()
            .padding(.top, Space.xl)
        }
    }

    private func macs(at now: Date) -> [InboxLayout.Mac] {
        hosts.map { host in
            InboxLayout.Mac(
                id: host.id, name: host.name, reachability: host.reachability(at: now), asOf: host.staleAsOf(at: now),
                sessions: host.sessions.sessions.values.map { s in
                    let card = s.listing.card
                    return InboxLayout.Session(id: s.id, status: card.status, updatedAt: card.updatedAt)
                })
        }
    }

    private func retry(_ mac: String?) {
        Haptics.tap()
        hosts.first { $0.id == mac }?.scene(active: true)
    }
}

/// A session in the Inbox: where it is, what it asks or how it ended, and Deny and Approve under a plain ask.
@MainActor
struct InboxRow: View {
    let host: HostModel
    let session: SessionModel
    let entry: InboxLayout.Entry
    let showMac: Bool
    var failed = false

    var body: some View {
        let s = session.summary
        let status = session.status
        let ask = session.requests.first
        let plain = ask.flatMap { SessionText.answersInPlace($0) && host.canApprove ? $0 : nil }
        let head = InboxText.headline(s, status, ask: ask)
        VStack(alignment: .leading, spacing: 0) {
            PushRow(value: Route.chat(mac: host.mac, agentId: session.id)) {
                SessionRowFrame(status: status, title: head.text, code: head.code, wraps: plain != nil, pinned: s.pinned == true, updatedAt: s.updatedAt,
                                ticking: status == .working, bottom: plain == nil ? 12 : 0) {
                    Self.place(InboxText.place(project: s.project, branch: s.branch ?? s.worktree, mac: showMac ? host.name : nil)).rowLine()
                }
            }
            .buttonStyle(.plain)
            if let plain {
                InboxAnswer(host: host, request: plain, locked: entry.locked, attempt: failed ? .failed(.allow) : .idle)
                    .id(plain.requestId)
                    .padding(.leading, Inset.textColumn)
                    .padding(.top, Space.s + 2)
                    .padding(.bottom, Space.m)
            }
        }
        .opacity(entry.dim)
    }

    static func place(_ parts: [InboxText.Part]) -> Text {
        let texts = parts.map { $0.code ? Text($0.text).font(.system(size: 12.5, design: .monospaced)) : Text($0.text) }
        return texts.dropFirst().reduce(texts.first ?? Text("")) { $0 + Text(" · ") + $1 }
    }
}

/// Deny and Approve, and what came of sending the answer. It sits beside its row's link, so a press can't also open the chat.
@MainActor
struct InboxAnswer: View {
    enum Attempt: Equatable {
        case idle
        case sending
        case sent(Decision)
        case failed(Decision)
    }

    let host: HostModel
    let request: PermissionRequest
    let locked: Bool
    @State private var attempt: Attempt

    init(host: HostModel, request: PermissionRequest, locked: Bool, attempt: Attempt = .idle) {
        self.host = host
        self.request = request
        self.locked = locked
        _attempt = State(initialValue: attempt)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            if case .sent(let decision) = attempt {
                Text(InboxText.answered(decision, mac: host.name)).font(.system(size: 13)).foregroundStyle(Palette.text2.color)
            } else {
                HStack(spacing: Space.s) {
                    Button("Deny") { send(.deny) }.buttonStyle(PillButtonStyle(.secondary, height: 32))
                    Button("Approve") { send(.allow) }.buttonStyle(PillButtonStyle(.primary, height: 32))
                }
                .disabled(locked || attempt == .sending)
                if case .failed(let decision) = attempt {
                    Button { send(decision) } label: {
                        (Text("Couldn't send your answer. ").foregroundStyle(Palette.failedText.color)
                            + Text("Try again").fontWeight(.semibold).foregroundStyle(Palette.text.color))
                            .font(.system(size: 13))
                    }
                    .buttonStyle(.plain)
                    .disabled(locked)
                }
            }
        }
    }

    private func send(_ decision: Decision) {
        attempt = .sending
        Haptics.tap()
        Task {
            do {
                try await host.answer(request, decision)
                attempt = .sent(decision)
                Haptics.success()
            } catch {
                attempt = .failed(decision)
                Haptics.warning()
            }
        }
    }
}

/// The Inbox before any Mac has answered: the shape of its groups, in the quiet fills.
@MainActor
struct InboxSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Bar(width: 220, height: 12, fill: Palette.fill2).padding(.leading, 20).padding(.top, Space.s)
            Bar(width: 72, height: 10, fill: Palette.fill2).padding(.leading, 32).padding(.top, 28).padding(.bottom, 9)
            VStack(spacing: 0) {
                row(0.78, 0.52, buttons: true)
                row(0.66, 0.44).rowDivider()
                row(0.72, 0.58).rowDivider()
            }
            .insetGroup()
            Bar(width: 56, height: 10, fill: Palette.fill2).padding(.leading, 32).padding(.top, 26).padding(.bottom, 9)
            VStack(spacing: 0) {
                row(0.60, 0.40)
                row(0.70, 0.48).rowDivider()
            }
            .insetGroup()
        }
        .accessibilityHidden(true)
    }

    private func row(_ title: Double, _ line: Double, buttons: Bool = false) -> some View {
        HStack(alignment: .top, spacing: Space.l) {
            Circle().fill(Palette.fill3.color).frame(width: 18, height: 18)
            VStack(alignment: .leading, spacing: Space.s) {
                GeometryReader { box in
                    VStack(alignment: .leading, spacing: Space.s) {
                        Bar(width: box.size.width * title, height: 12, fill: Palette.fill3)
                        Bar(width: box.size.width * line, height: 10, fill: Palette.fill2)
                    }
                }
                .frame(height: 30)
                if buttons {
                    HStack(spacing: Space.s) {
                        Capsule().fill(Palette.fill2.color).frame(width: 64, height: 28)
                        Capsule().fill(Palette.fill2.color).frame(width: 84, height: 28)
                    }
                    .padding(.top, Space.xs)
                }
            }
            .padding(.top, 3)
        }
        .padding(.leading, 17)
        .padding(.vertical, Space.l)
        .padding(.trailing, Space.l)
    }

    private struct Bar: View {
        let width: CGFloat
        let height: CGFloat
        let fill: Token

        var body: some View {
            Capsule().fill(fill.color).frame(width: width, height: height)
        }
    }
}
