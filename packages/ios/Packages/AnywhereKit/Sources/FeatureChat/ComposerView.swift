import ChatModel
import DesignSystem
import Stores
import SwiftUI
import Wire
#if os(iOS)
    import PhotosUI
#endif

/// Under the transcript, top to bottom: the goal, a `/btw` answer, the background and queue trays, then the box (or
/// the ask that replaces it) and the footer.
@MainActor
struct ComposerView: View {
    @Bindable var store: ChatStore
    let machine: String
    /// Shows a call in the transcript.
    let show: (String) -> Void
    @Binding var mapOpen: Bool
    @FocusState private var focused: Bool
    @Environment(\.openURL) private var openURL
    #if os(iOS)
        @State private var photos: [PhotosPickerItem] = []
    #endif

    private var caps: Capabilities { store.capabilities }
    private var cursor: Int { store.draft.text.utf8.count }

    var body: some View {
        let session = store.session
        VStack(spacing: Space.s) {
            if let goal = store.summary?.goal {
                GoalStripView(goal: goal, pauses: store.adapter.pausesGoal) { store.goal($0) }
            }
            if let btw = store.btw {
                BtwCard(question: btw.question, answer: btw.answer, error: btw.error) { store.closeBtw() }
            }
            let tasks = inBackground(session.transcript.turns[...]).map(\.call)
            if !tasks.isEmpty {
                BackgroundTray(
                    tasks: tasks, stops: Background.stops(chatOnly: store.summary?.chatOnly ?? false, interrupt: caps.interrupt, busy: store.busy, tasks),
                    stop: { store.stopBackground() }, show: show)
            }
            let queued = Queue.waiting(session.outgoing.list)
            if !queued.isEmpty {
                QueueTray(
                    sends: queued, paused: session.outgoing.paused(session.transcript), canSteer: store.busy && caps.sendWhileWorking,
                    resume: { store.resume() }, steer: { store.steerQueued($0) }, edit: { store.editQueued($0) }, remove: { store.removeQueued($0) })
            }
            switch store.mode {
            case .prompt:
                box
            case .permission(let answerable):
                if let ask = store.ask {
                    PermissionCard(ask: ask, who: who(store.provider), feedback: store.feedback, answerable: answerable, allow: { store.allow() },
                                   deny: { store.deny($0) })
                }
            case .question(let answerable):
                if let wizard = store.wizard {
                    QuestionWizard(wizard: wizard, answerable: answerable, change: { store.wizard($0) }, skip: { store.skipQuestions() })
                }
            case .viewOnly:
                Text("View only").font(ChatFont.small).foregroundStyle(Palette.text3.color).frame(maxWidth: .infinity, minHeight: 44)
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
        .confirmationDialog(
            store.settingsMenu == .effort ? "Effort" : "Model",
            isPresented: Binding(get: { store.settingsMenu != nil }, set: { if !$0 { store.settingsMenu = nil } })
        ) {
            if let agent = store.summary { settingsChoices(agent) }
        }
    }

    // MARK: Box

    private var box: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !store.draft.images.isEmpty { AttachmentStrip(images: store.draft.images) { store.detach($0) } }
            TextField(
                Composer.placeholder(working: store.busy, stopping: store.stopping, canQueue: caps.sendWhileWorking), text: $store.draft.text,
                axis: .vertical
            )
            .lineLimit(1...Composer.maxRows)
            .font(ChatFont.bubble)
            .textFieldStyle(.plain)
            .focused($focused)
            .padding(.horizontal, 4)
            .padding(.top, 4)
            .onChange(of: store.draft.text) { _, _ in
                store.suggest.selected = 0
                store.drafted(cursor: cursor)
            }
            .onKeyPress(.return, phases: .down) { press in
                guard !press.modifiers.contains(.shift) else { return .ignored }
                if let rows = store.suggestions(cursor: cursor), rows.count > 0 {
                    store.pick(store.suggest.selected, cursor: cursor)
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
                guard let rows = store.suggestions(cursor: cursor), rows.count > 0 else { return .ignored }
                store.suggest.step(down: press.key == .downArrow, total: rows.count)
                return .handled
            }
            bar
        }
        .padding(8)
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Palette.separator.color))
        .overlay(alignment: .top) {
            if let rows = store.suggestions(cursor: cursor), showsList(rows) {
                SuggestList(
                    rows: rows, selected: store.suggest.selected, provider: store.provider,
                    signsIn: { store.suggest.signsIn(store.agentId, $0) },
                    acting: { name in store.suggest.acting[store.agentId].flatMap { $0.server == name ? $0.action : nil } },
                    pick: { store.pick($0, cursor: cursor) }
                )
                // Opens upward over the transcript, its bottom just over the box.
                .frame(height: SuggestList.maxHeight, alignment: .bottom)
                .offset(y: -SuggestList.maxHeight - 6)
            }
        }
    }

    private func showsList(_ rows: SuggestRows) -> Bool {
        if case .servers = rows { return true }
        return rows.count > 0
    }

    private var bar: some View {
        HStack(spacing: 2) {
            attachButton
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
            if Composer.stopBesideSteer(store.action, interrupt: caps.interrupt) { GhostStop { store.stop() } }
            SendButton(action: store.action, empty: store.draft.isEmpty, allowed: caps.allows(store.action)) {
                if store.action == .stop { Haptics.warning() }
                store.press()
            }
            .keyboardShortcut(.return, modifiers: .command)
        }
    }

    @ViewBuilder
    private var attachButton: some View {
        #if os(iOS)
            PhotosPicker(selection: $photos, maxSelectionCount: 4, matching: .images) {
                Image(systemName: "plus").font(.system(size: 15, weight: .medium)).foregroundStyle(Palette.text2.color).frame(width: 32, height: 32)
            }
            .accessibilityLabel("Attach images")
            .onChange(of: photos) { _, items in
                guard !items.isEmpty else { return }
                photos = []
                for item in items {
                    let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                    Task { @MainActor in
                        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
                        store.attach(data, name: "\(UUID().uuidString.prefix(8)).\(ext)", thumbnail: data)
                    }
                }
            }
        #endif
    }

    @ViewBuilder
    private var agentsChip: some View {
        let map = spawned(store.session.transcript)
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
                .contentShape(Rectangle())
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
            } else if let hint = Composer.hint(store.mode, working: store.busy, stopping: store.stopping, empty: store.draft.isEmpty, canQueue: caps.sendWhileWorking) {
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
