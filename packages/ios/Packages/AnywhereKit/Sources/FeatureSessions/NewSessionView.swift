import AttachUI
import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The new session sheet: the prompt in large type, and at the bottom the chips for where it runs and with what, over
/// Start. Starting shows pocketd's steps, then opens the chat; hidden, the start goes on as a row under its project.
/// Offline, the prompt can still be written, and Start waits for the Mac.
@MainActor
struct NewSessionView: View {
    let host: HostModel
    let mac: String
    @State private var confirmingFull = false
    @State private var confirmingStart = false
    @State private var draft: NewSessionDraft
    @State private var images: LaunchImages
    @State private var requestId: String?
    @State private var worktreeOptions = false
    @State private var providersError: String?
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss

    init(host: HostModel, mac: String, project: String?, place: NewSessionDraft.Place?, prompt: String = "", images: LaunchImages? = nil) {
        self.host = host
        self.mac = mac
        var draft = NewSessionDraft(project: project ?? host.workspace.projects.first?.path ?? "", place: place ?? .newWorktree)
        draft.seed = Int.random(in: 0..<1_000_000)
        draft.prompt = prompt
        _draft = State(initialValue: draft)
        _images = State(initialValue: images ?? LaunchImages(files: host.files, macName: host.name))
    }

    private var workspace: WorkspaceStore { host.workspace }
    private var project: Project? { workspace.project(draft.project) }
    private var mainTree: String { Self.main(of: project) ?? draft.project }
    private var providers: AgentProviders? { workspace.providers }
    private var info: ProviderInfo? { providers?.info(draft.provider.rawValue) }
    private var neverAsks: Bool { info?.neverAsks == true }
    private var taken: Set<String> { Places.taken(project: project, branches: workspace.branches[mainTree]) }
    private var canSpawn: Bool { host.hello?.scopes?.contains(.spawn) ?? true }
    private var problem: String? {
        canSpawn ? draft.problem(taken: taken) : "Your Mac doesn't let this phone start sessions"
    }
    private var places: [Places.Option] { Places.options(canManage: workspace.canManage, canSpawn: canSpawn) }
    private var existing: Places.Option? {
        project.flatMap { Places.existing(draft.place, project: $0, locals: workspace.locals, names: workspace.names) }
    }

    var body: some View {
        Group {
            if let requestId {
                LaunchProgress(host: host, mac: mac, requestId: requestId)
            } else {
                compose
            }
        }
        #if os(iOS)
            .toolbar(.hidden, for: .navigationBar)
        #endif
        .sheet(isPresented: $worktreeOptions) {
            NavigationStack {
                Form {
                    WorktreeFields(
                        draft: $draft, branches: workspace.branches[mainTree], branchError: workspace.branchErrors[mainTree],
                        retryBranches: { Task { await loadBranches() } }, canChooseSource: workspace.canManage, taken: taken)
                }
                .navigationTitle("New Worktree")
                #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { worktreeOptions = false } } }
            }
            .presentationDetents([.medium, .large])
        }
        .task { await load() }
        .onChange(of: online) { _, online in
            if online && providers == nil {
                Task { await load() }
            } else if online && workspace.branchErrors[mainTree] != nil {
                Task { await loadBranches() }
            }
        }
        #if os(iOS)
            .onAppear { focused = true }
        #endif
        .onChange(of: draft.provider) { _, provider in Task { try? await workspace.loadAccounts(provider: provider.rawValue) } }
        .onChange(of: draft.project) { _, _ in Task { await loadBranches() } }
    }

    // MARK: Compose

    private var compose: some View {
        VStack(spacing: 0) {
            header
            ScrollView {
                VStack(alignment: .leading, spacing: Space.l) {
                    prompt
                    if !images.list.isEmpty {
                        AttachmentStrip(images: images.list, locked: false, detach: { images.detach($0) }, retry: { images.retry($0) })
                    }
                    AttachMenu(enabled: true, attach: attach, unreadable: { images.unreadable() })
                        .frame(width: 40, height: 40)
                        .background(Palette.fill3.color, in: Circle())
                }
                .padding(.horizontal, 20)
                .padding(.top, Space.xl)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollBounceBehavior(.basedOnSize)
            VStack(alignment: .leading, spacing: 0) {
                note
                chips
                startButton
            }
            .frame(maxWidth: 744)
            .padding(.bottom, Space.s)
        }
        .frame(maxWidth: .infinity)
        .background(Palette.surface.color.ignoresSafeArea())
    }

    private var header: some View {
        ZStack {
            Text("New session").font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color)
            HStack {
                Button { dismiss() } label: { LucideIcon(.close, size: 16).foregroundStyle(Palette.text.color).frame(width: 36, height: 36) }
                    .buttonStyle(.plain)
                    .background(Palette.fill3.color, in: Circle())
                    .accessibilityLabel("Close")
                Spacer()
            }
        }
        .padding(.horizontal, Space.l)
        .padding(.top, 11)
        .frame(height: 55)
    }

    private var prompt: some View {
        TextField("Prompt", text: $draft.prompt, prompt: Text("What should we build next?").foregroundStyle(Palette.text4.color), axis: .vertical)
            .font(.system(size: 28, weight: .bold))
            .foregroundStyle(Palette.text.color)
            .textFieldStyle(.plain)
            .focused($focused)
            #if os(iOS)
                .pastesImages(attach)
            #endif
            .padding(.horizontal, 4)
    }

    @ViewBuilder
    private var note: some View {
        if let wait = Self.waiting(online: online, providers: providers != nil, failed: providersError != nil, mac: host.name, asOf: host.staleAsOf(at: .now)) {
            noteLine(wait, color: Palette.text2)
        } else if let providersError {
            HStack {
                Text(providersError).font(.system(size: 13)).foregroundStyle(Palette.failedText.color)
                Spacer(minLength: Space.s)
                Button { Task { await load() } } label: { Text("Try Again").hitTarget() }.font(.system(size: 13, weight: .semibold))
            }
            .padding(.horizontal, 24)
            .padding(.bottom, Space.s)
        } else if let problem = problem ?? images.notice {
            noteLine(problem, color: Palette.failedText)
        }
    }

    private func noteLine(_ text: String, color: Token) -> some View {
        Text(text)
            .font(.system(size: 13))
            .foregroundStyle(color.color)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 24)
            .padding(.bottom, Space.m)
    }

    private var chips: some View {
        FlowLayout(spacing: Space.s) {
            Chip(text: host.name) { if !online { Circle().fill(Palette.text4.color).frame(width: 7, height: 7) } }
            projectMenu
            placeChip
            if draft.place == .newWorktree {
                Button { worktreeOptions = true } label: { Chip(text: branchLabel, mono: true) }.buttonStyle(.plain)
            }
            agentMenu
            modelMenu
            optionsMenu
        }
        .padding(.horizontal, Space.l)
    }

    private func attach(_ data: Data, _ name: String, _ thumbnail: Data?) {
        images.attach(data, name: name, thumbnail: thumbnail)
    }

    private var startButton: some View {
        let blocked = requestId != nil || !online || providers == nil || problem != nil || images.blocksStart
        return Button {
            if draft.confirmsStart(neverAsks: neverAsks) {
                Haptics.warning()
                confirmingStart = true
            } else {
                start()
            }
        } label: {
            Text("Start")
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle((blocked ? Palette.text4 : Palette.onText).color)
                .frame(maxWidth: .infinity)
                .frame(height: 52)
                .background((blocked ? Palette.fill3 : Palette.text).color, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(blocked)
        .padding(.horizontal, 20)
        .padding(.top, Space.l)
        .confirmsFullAccess($confirmingStart, who: AgentName.label(draft.provider.rawValue), action: "Start with Full", confirm: start)
    }

    private var projectMenu: some View {
        Menu {
            Picker("Project", selection: Binding(get: { draft.project }, set: { draft.pick(project: $0) })) {
                ForEach(workspace.projects) { Text($0.name).tag($0.path) }
            }
        } label: {
            Chip(text: project?.name ?? "Project")
        }
        .chipMenu()
        .disabled(workspace.projects.count < 2)
    }

    /// The desktop's footer choice: a new Worktree or Local, or the workspace a session was asked for from.
    @ViewBuilder
    private var placeChip: some View {
        if let existing {
            Chip(text: existing.label)
        } else {
            let place = places.first { $0.place == draft.place }
            Menu {
                // Buttons, not a Picker: only a button's second text shows as the item's subtitle.
                ForEach(places) { option in
                    Button { draft.place = option.place } label: {
                        Label { Text(option.label) } icon: { Image.lucide(option.place == draft.place ? .check : option.icon, size: 16) }
                        if let hint = option.hint { Text(hint) }
                    }
                }
            } label: {
                Chip(text: place?.label ?? "Worktree")
            }
            .chipMenu()
        }
    }

    private var agentMenu: some View {
        Menu {
            Picker("Agent", selection: Binding(get: { draft.provider }, set: { draft.pick(provider: $0) })) {
                ForEach(providers?.providers.filter(\.available) ?? [], id: \.id) { Text(AgentName.label($0.id)).tag(ProviderId(rawValue: $0.id)) }
            }
        } label: {
            Chip(text: AgentName.full(draft.provider.rawValue) ?? AgentName.label(draft.provider.rawValue))
        }
        .chipMenu()
    }

    private var modelMenu: some View {
        let models = NewSessionDraft.models(draft.provider)
        let efforts = info?.efforts ?? []
        let model = draft.model.map(modelName) ?? "Default model"
        let effortWord = AgentName.effort(draft.provider.rawValue)
        return Menu {
            if !models.isEmpty {
                Picker("Model", selection: $draft.model) {
                    Text("Default").tag(String?.none)
                    ForEach(models, id: \.self) { Text(modelName($0)).tag(Optional($0)) }
                }
            }
            if !efforts.isEmpty {
                Picker(effortWord, selection: $draft.effort) {
                    Text("Default \(effortWord)").tag(String?.none)
                    ForEach(efforts, id: \.self) { Text($0.capitalized).tag(Optional($0)) }
                }
                .pickerStyle(.menu)
            }
        } label: {
            Chip(text: draft.effort.map { "\(model) · \($0.capitalized)" } ?? model)
        }
        .chipMenu()
        .disabled(models.isEmpty && efforts.isEmpty)
    }

    /// Whether it plans first or only chats, which account it runs as, and what it may do unasked.
    private var optionsMenu: some View {
        Menu {
            if info?.plan == true { Toggle("Plan First", isOn: $draft.plan) }
            Toggle("Chat Only", isOn: $draft.chat)
            if let accounts = workspace.accounts[draft.provider.rawValue]?.filter(\.signedIn), !accounts.isEmpty {
                // pocketd picks the project's account, else the Mac's default.
                Picker("Account", selection: $draft.account) {
                    Text("Default Account").tag(String?.none)
                    ForEach(accounts) { Text(SessionText.account($0)).tag(Optional($0.id)) }
                }
                .pickerStyle(.menu)
            }
            accessSection
        } label: {
            Chip(text: "Options") { LucideIcon(.sliders, size: 16) }
        }
        .chipMenu()
        .confirmsFullAccess($confirmingFull, who: AgentName.label(draft.provider.rawValue)) { draft.access = .full }
    }

    /// An agent that never asks before a tool has only Full, so nothing to pick.
    @ViewBuilder
    private var accessSection: some View {
        if neverAsks {
            Button {} label: { Text("Access: \(AccessPill.label(Access.full.rawValue))") }.disabled(true)
        } else {
            Section("Access") {
                AccessChoices(choices: AccessPill.choices(launching: true), picked: draft.access.rawValue) { value in
                    if AccessPill.confirms(value, from: draft.access.rawValue) {
                        Haptics.warning()
                        confirmingFull = true
                    } else {
                        draft.access = Access(rawValue: value)
                    }
                }
            }
        }
    }

    /// The base a new branch starts from, or the branch or pull request it opens.
    private var branchLabel: String {
        switch draft.source {
        case .new: return draft.base ?? Places.defaultBase(project: project, branches: workspace.branches[mainTree]) ?? "Default"
        case .branch(let b): return b.isEmpty ? "Pick a branch" : b
        case .pr(let typed): return typed.isEmpty ? "Pick a pull request" : "PR \(typed)"
        }
    }

    private func load() async {
        providersError = nil
        do {
            let providers = try await workspace.loadProviders()
            draft.access = NewSessionDraft.defaultAccess(mac: providers.defaultAccess)
            if !providers.providers.contains(where: { $0.id == draft.provider.rawValue && $0.available }),
               let first = providers.providers.first(where: \.available) {
                draft.pick(provider: ProviderId(rawValue: first.id))
            }
        } catch {
            // Offline says so on its own line, and loads again once the Mac is back.
            if online && providers == nil { providersError = ErrorText.describe(error, mac: host.name) }
        }
        _ = try? await workspace.loadAccounts(provider: draft.provider.rawValue)
        await loadBranches()
    }

    private func loadBranches() async {
        guard workspace.canManage, !draft.project.isEmpty else { return }
        _ = try? await workspace.loadBranches(tree: mainTree)
    }

    private func start() {
        let id = WorkspaceStore.newRequestId()
        let plan = draft.plan(
            taken: taken, localId: Sidebar.newLocalId(), localName: workspace.nextLocalName(project: draft.project), neverAsks: neverAsks,
            images: images.list.paths)
        workspace.add(plan, requestId: id, title: Self.title(draft.prompt), place: Self.key(plan.spec.checkout, makesWorktree: plan.worktree != nil))
        requestId = id
        workspace.runKept(id)
    }

    private var online: Bool { host.link == .online }

    static func main(of project: Project?) -> String? {
        project.map { $0.worktrees.first(where: \.isMain)?.path ?? $0.path }
    }

    /// Why Start waits, when it waits on the Mac rather than on the draft.
    nonisolated static func waiting(online: Bool, providers: Bool, failed: Bool, mac: String, asOf: Int? = nil, timeZone: TimeZone = .current) -> String? {
        if !online {
            let seen = asOf.map { " · last seen \(TimeText.clock(ms: $0, timeZone: timeZone))" } ?? ""
            return "\(mac) is offline\(seen). Start turns on when it's back."
        }
        if !providers && !failed { return "Loading agents from \(mac)…" }
        return nil
    }

    /// A start's row title: the prompt's first line.
    nonisolated static func title(_ prompt: String) -> String {
        prompt.split(whereSeparator: \.isNewline).first.map { $0.trimmingCharacters(in: .whitespaces) } ?? ""
    }

    /// The worktree or Local a plan starts in when it isn't a new worktree, as `.worktree` and `.local` routes key them.
    /// `makesWorktree`: a `worktree.create` runs first, and the checkout is only a stand-in until it has.
    nonisolated static func key(_ checkout: Checkout, makesWorktree: Bool) -> String? {
        switch checkout {
        case .worktree(let path, _): makesWorktree ? nil : path
        case .local(let id): id
        case .new: nil
        }
    }
}

/// A choice or a fact about the session, as a pill: the Mac, where it runs, what runs it.
@MainActor
private struct Chip<Leading: View>: View {
    let text: String
    var mono = false
    @ViewBuilder let leading: Leading

    var body: some View {
        HStack(spacing: 5) {
            leading
            Text(text).font(mono ? .system(size: 14, design: .monospaced) : .system(size: 15)).lineLimit(1)
        }
        .foregroundStyle(Palette.text.color)
        .padding(.horizontal, 12)
        .frame(height: 34)
        .background(Palette.fill3.color, in: Capsule())
        .contentShape(Capsule())
    }
}

extension View {
    /// A menu drawn as its label alone, in a pill, without the system's menu button around it.
    fileprivate func chipMenu() -> some View {
        menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).menuOrder(.fixed)
    }
}

extension Chip where Leading == EmptyView {
    init(text: String, mono: Bool = false) {
        self.init(text: text, mono: mono) { EmptyView() }
    }
}
