import ChatModel
import DesignSystem
import MarkdownView
import Stores
import SwiftUI
import Wire

/// Under the transcript, top to bottom: the goal, a `/btw` answer, the background and queue trays, then the box (or
/// the ask that replaces it) and the footer.
@MainActor
struct ComposerView: View {
    /// The coordinate space the chat screen names, which the suggest list measures its room in.
    static let space = "chat-screen"

    @Bindable var store: ChatStore
    let machine: String
    /// The chat screen's height, which caps the input's rows.
    let height: CGFloat
    /// Bumped by each tap on the transcript, which closes the suggest list.
    let outsideTaps: Int
    /// Shows a call in the transcript.
    let show: (String) -> Void
    @Binding var mapOpen: Bool
    let openLink: (MarkdownLink) -> Void
    @FocusState private var focused: Bool
    @Environment(\.openURL) private var openURL
    @Environment(\.dismiss) private var dismiss
    @State private var keyboard = HardwareKeyboard.attached
    /// The draft the suggest list was closed on; it opens again once the draft changes.
    @State private var closedOn: String?
    /// How far the box's top is from the screen's: the room the suggest list opens into.
    @State private var room = SuggestList.maxHeight
    #if os(iOS)
        @State private var selection: TextSelection?
    #endif

    private var caps: Capabilities { store.capabilities }

    /// Where the caret is in the draft, in UTF-8 bytes; the end where the platform can't say.
    private var cursor: Int {
        let text = store.draft.text
        #if os(iOS)
            if case .selection(let range)? = selection?.indices, range.upperBound <= text.endIndex,
                let upper = range.upperBound.samePosition(in: text.utf8)
            {
                return text.utf8.distance(from: text.utf8.startIndex, to: upper)
            }
        #endif
        return text.utf8.count
    }

    var body: some View {
        let _ = BodyCount.hit("ComposerView")
        VStack(spacing: Space.s) {
            if let goal = store.summary?.goal {
                GoalStripView(goal: goal, pauses: store.adapter.pausesGoal) { store.goal($0) }
            }
            if let btw = store.btw {
                BtwCard(question: btw.question, answer: btw.answer, error: btw.error) { store.closeBtw() }
            }
            let tasks = store.background
            if !tasks.isEmpty {
                BackgroundTray(
                    tasks: tasks, stops: Background.stops(chatOnly: store.summary?.chatOnly ?? false, interrupt: caps.interrupt, busy: store.busy, tasks),
                    stop: { store.stopBackground() }, show: show)
            }
            QueueSlot(store: store, canSteer: store.busy && caps.sendWhileWorking && store.online)
            switch store.mode {
            case .prompt:
                box
            case .permission(let answerable):
                if let ask = store.ask {
                    PermissionCard(
                        ask: ask, who: who(store.provider), feedback: store.feedback, answerable: answerable, sent: store.answering == ask.requestId,
                        online: store.online, allow: { store.allow() }, deny: { store.deny($0) }
                    )
                    .id(ask.requestId)
                }
            case .question(let answerable):
                if let wizard = store.wizard {
                    QuestionWizard(
                        wizard: wizard, answerable: answerable, online: store.online, who: who(store.provider), change: { store.wizard($0) },
                        skip: { store.skipQuestions() }, openLink: openLink)
                }
            case .viewOnly:
                Text(Composer.viewOnly(mac: machine)).font(ChatFont.small).foregroundStyle(Palette.text3.color).multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity, minHeight: 44)
            case .closed:
                closedNote
            }
            footer
        }
        .animation(.easeOut(duration: 0.2), value: store.mode)
        .padding(.horizontal, Space.m)
        .padding(.bottom, Space.s)
        .frame(maxWidth: 744)
        .frame(maxWidth: .infinity)
        .onChange(of: store.authUrl) { _, url in
            guard let url, let link = URL(string: url) else { return }
            openURL(link)
            store.openedAuthUrl()
        }
        .onChange(of: outsideTaps) { _, _ in closedOn = store.draft.text }
        .onReceive(HardwareKeyboard.changes) { keyboard = HardwareKeyboard.attached }
        .confirmationDialog(
            store.settingsMenu == .effort ? "Effort" : "Model",
            isPresented: Binding(get: { store.settingsMenu != nil }, set: { if !$0 { store.settingsMenu = nil } })
        ) {
            if let agent = store.summary { settingsChoices(agent) }
        }
    }

    private var closedNote: some View {
        VStack(spacing: 0) {
            Text("This session was closed").font(ChatFont.small.weight(.semibold)).foregroundStyle(Palette.text2.color)
            Button { dismiss() } label: { Text("Go Back").hitTarget() }
                .buttonStyle(.plain).font(ChatFont.small).foregroundStyle(Palette.link.color)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, Space.s)
    }

    // MARK: Box

    private var box: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !store.draft.images.isEmpty {
                AttachmentStrip(images: store.draft.images, locked: store.armed, detach: { store.detach($0) }, retry: { store.retryUpload($0) })
            }
            input
                .lineLimit(1...Composer.rows(height: Double(height)))
                .font(ChatFont.bubble)
                .textFieldStyle(.plain)
                .focused($focused)
                .disabled(store.armed)
                .padding(.horizontal, 4)
                .padding(.top, 4)
                .onChange(of: store.draft.text) { _, _ in
                    store.suggest.selected = 0
                    store.drafted(cursor: cursor)
                }
                .onKeyPress(.return, phases: .down) { press in
                    guard !press.modifiers.contains(.shift) else { return .ignored }
                    if let rows = shownRows, rows.count > 0 {
                        pick(store.suggest.selected)
                    } else {
                        store.send(queue: press.modifiers.contains(.command))
                    }
                    return .handled
                }
                .onKeyPress(.escape) {
                    guard store.draft.isEmpty, store.action == .stop else { return .ignored }
                    store.stop()
                    return .handled
                }
                .onKeyPress(keys: [.upArrow, .downArrow]) { press in
                    guard let rows = shownRows, rows.count > 0 else { return .ignored }
                    store.suggest.step(down: press.key == .downArrow, total: rows.count)
                    return .handled
                }
            bar
        }
        .padding(8)
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Palette.separator.color))
        .background {
            GeometryReader { g in
                let top = g.frame(in: .named(Self.space)).minY
                Color.clear.onAppear { room = top - 6 }.onChange(of: top) { _, top in room = top - 6 }
            }
        }
        .overlay(alignment: .top) {
            if let rows = shownRows {
                let height = max(min(room, SuggestList.maxHeight), SuggestList.rowHeight)
                SuggestList(
                    rows: rows, selected: store.suggest.selected, provider: store.provider,
                    signsIn: { store.suggest.signsIn(store.agentId, $0) },
                    acting: { name in store.suggest.acting[store.agentId].flatMap { $0.server == name ? $0.action : nil } },
                    pick: { pick($0) }, height: height
                )
                // Opens upward over the transcript, its bottom just over the box.
                .frame(height: height, alignment: .bottom)
                .offset(y: -height - 6)
            }
        }
    }

    @ViewBuilder
    private var input: some View {
        let placeholder = Composer.placeholder(working: store.busy, stopping: store.stopping, canQueue: caps.sendWhileWorking)
        #if os(iOS)
            TextField(placeholder, text: $store.draft.text, selection: $selection, axis: .vertical)
        #else
            TextField(placeholder, text: $store.draft.text, axis: .vertical)
        #endif
    }

    /// The suggest list's rows, unless it was closed on this draft.
    private var shownRows: SuggestRows? {
        guard closedOn != store.draft.text, let rows = store.suggestions(cursor: cursor) else { return nil }
        if case .servers = rows { return rows }
        return rows.count > 0 ? rows : nil
    }

    private func pick(_ ix: Int) {
        guard let at = store.pick(ix, cursor: cursor) else { return }
        #if os(iOS)
            let text = store.draft.text
            selection = TextSelection(insertionPoint: text.utf8.index(text.utf8.startIndex, offsetBy: min(at, text.utf8.count)))
        #endif
    }

    private var bar: some View {
        HStack(spacing: 2) {
            AttachMenu(enabled: !store.armed) { data, name, thumbnail in store.attach(data, name: name, thumbnail: thumbnail) }
            if let agent = store.summary {
                AccessPillView(
                    access: agent.access ?? "settings", editable: agent.chatOnly == true && store.hasCap(Caps.access),
                    max: store.providers?.phoneMaxAccess ?? "full") { store.settings(access: $0) }
            }
            agentsChip
            Spacer(minLength: 0)
            if let agent = store.summary {
                if let (used, window) = ContextMeter.context(agent) { ContextRing(used: used, window: window) }
                ModelPillView(agent: agent, efforts: efforts(agent), editable: store.hasCap(Caps.settings)) { model, effort in
                    store.settings(model: model, effort: effort)
                }
            }
            HStack(spacing: Space.s) {
                if Composer.stopBesideSteer(store.action, interrupt: caps.interrupt) { GhostStop(enabled: store.online) { store.stop() } }
                SendButton(
                    action: store.action, empty: store.draft.isEmpty, allowed: caps.allows(store.action), enabled: store.online, armed: store.armed,
                    queue: store.action == .steer && !store.armed ? { store.send(queue: true) } : nil
                ) {
                    if store.action == .stop { Haptics.warning() }
                    store.press()
                }
                .keyboardShortcut(.return, modifiers: .command)
            }
        }
    }

    @ViewBuilder
    private var agentsChip: some View {
        let map = store.agents
        let (live, label) = AgentMap.chipLabel(map)
        if !map.isEmpty && (live || mapOpen) {
            Button {
                Haptics.tap()
                mapOpen = true
            } label: {
                HStack(spacing: 5) {
                    Circle().fill((live ? Palette.success : Palette.text4).color).frame(width: 6, height: 6)
                    Text(label)
                }
                .font(ChatFont.caption)
                .foregroundStyle(Palette.text2.color)
                .padding(.horizontal, 8)
                .frame(height: 28)
                .hitTarget()
            }
            .buttonStyle(.plain)
        }
    }

    // MARK: Footer

    private var footer: some View {
        HStack(spacing: Space.s) {
            if let agent = store.summary {
                if let branch = agent.branch, !branch.isEmpty {
                    Label(branch, systemImage: "arrow.triangle.branch").font(ChatFont.monoSmall).lineLimit(1).truncationMode(.middle)
                }
                if agent.chatOnly == true, let current = currentAccount(agent) {
                    AccountChip(accounts: providerAccounts(agent), current: current, provider: agent.provider, machine: machine) { store.switchAccount($0) }
                }
            }
            Spacer(minLength: 0)
            if let notice = store.notice {
                Text(notice).foregroundStyle(Palette.waitingText.color).lineLimit(2)
            } else if let hint = Composer.hint(
                store.mode, working: store.busy, stopping: store.stopping, empty: store.draft.isEmpty, canQueue: caps.sendWhileWorking, keyboard: keyboard,
                mac: machine)
            {
                Text(hint).lineLimit(1)
            }
        }
        .font(.system(size: 11.5))
        .foregroundStyle(Palette.text4.color)
        .padding(.horizontal, 4)
    }

    private func providerAccounts(_ agent: AgentSummary) -> [Account] {
        store.accounts.filter { $0.provider == agent.provider }
    }

    /// The account the agent runs on, when the Mac has several to switch between.
    private func currentAccount(_ agent: AgentSummary) -> Account? {
        let all = providerAccounts(agent)
        guard all.count > 1 else { return nil }
        return all.first { $0.id == agent.account } ?? all.first { $0.isDefault == true } ?? all.first
    }

    private func efforts(_ agent: AgentSummary) -> [String] {
        store.providers?.providers.first { $0.id == agent.provider }?.efforts ?? []
    }

    @ViewBuilder
    private func settingsChoices(_ agent: AgentSummary) -> some View {
        if store.settingsMenu == .effort {
            ForEach(efforts(agent), id: \.self) { e in Button(ModelPill.effortLabel(e)) { store.settings(effort: e) } }
        } else {
            ForEach(ModelPill.aliases(agent.provider), id: \.self) { m in Button(modelName(m)) { store.settings(model: m) } }
        }
    }
}

/// The queue tray; its own view, so only it reads the session that every stream frame changes.
@MainActor
private struct QueueSlot: View {
    let store: ChatStore
    let canSteer: Bool

    var body: some View {
        let session = store.session
        let queued = Queue.waiting(session.outgoing.list)
        if !queued.isEmpty {
            QueueTray(
                sends: queued, paused: session.outgoing.paused(session.transcript), canSteer: canSteer,
                resume: { store.resume() }, steer: { store.steerQueued($0) }, edit: { store.editQueued($0) }, remove: { store.removeQueued($0) })
        }
    }
}
