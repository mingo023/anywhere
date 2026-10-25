import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The new session sheet, prompt first as on the desktop: what to build in a composer whose pills pick the agent, its
/// model and access, over where it runs. Starting shows pocketd's steps, then opens the chat.
@MainActor
struct NewSessionView: View {
    let host: HostModel
    let mac: String
    /// The workspace the page was opened from, offered beside a new Worktree or Local.
    let opened: NewSessionDraft.Place?
    @State private var draft: NewSessionDraft
    @State private var requestId: String?
    @State private var worktreeOptions = false
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    init(host: HostModel, mac: String, project: String?, place: NewSessionDraft.Place?) {
        self.host = host
        self.mac = mac
        opened = place
        var draft = NewSessionDraft(project: project ?? host.workspace.projects.first?.path ?? "", place: place ?? .newWorktree)
        draft.seed = Int.random(in: 0..<1_000_000)
        _draft = State(initialValue: draft)
    }

    private var workspace: WorkspaceStore { host.workspace }
    private var project: Project? { workspace.project(draft.project) }
    private var mainTree: String { Self.main(of: project) ?? draft.project }
    private var providers: AgentProviders? { workspace.providers }
    private var maxAccess: String { providers?.phoneMaxAccess ?? Access.ask.rawValue }
    private var info: ProviderInfo? { providers?.providers.first { $0.id == draft.provider.rawValue } }
    private var taken: Set<String> { Places.taken(project: project, branches: workspace.branches[mainTree]) }
    private var canSpawn: Bool { host.hello?.scopes?.contains(.spawn) ?? true }
    private var problem: String? {
        canSpawn ? draft.problem(taken: taken, maxAccess: maxAccess) : "Your Mac doesn't let this phone start sessions"
    }
    private var places: [Places.Option] {
        project.map { Places.options(project: $0, opened: opened, locals: workspace.locals, names: workspace.names, canManage: workspace.canManage, canSpawn: canSpawn) } ?? []
    }

    var body: some View {
        Group {
            if let requestId {
                Form {
                    CreationProgress(creation: workspace.creations[requestId]) {
                        if let creation = workspace.creations[requestId] { draft.resume(after: creation) }
                        workspace.forget(creation: requestId)
                        self.requestId = nil
                    }
                }
                .formStyle(.grouped)
            } else {
                compose
            }
        }
        .navigationTitle("New Session")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") {
                    if let requestId { workspace.forget(creation: requestId) }
                    dismiss()
                }
            }
        }
        .sheet(isPresented: $worktreeOptions) {
            NavigationStack {
                Form {
                    WorktreeFields(draft: $draft, branches: workspace.branches[mainTree], canChooseSource: workspace.canManage, taken: taken)
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
        .onAppear { focused = true }
        .onChange(of: draft.provider) { _, provider in Task { try? await workspace.loadAccounts(provider: provider.rawValue) } }
        .onChange(of: draft.project) { _, _ in
            if draft.place != .newLocal { draft.place = .newWorktree }
            Task { await loadBranches() }
        }
    }

    // MARK: Compose

    private var compose: some View {
        VStack(spacing: Space.m) {
            Spacer()
            Text("What should we build next?")
                .font(Typography.title)
                .foregroundStyle(Palette.text.color)
                .multilineTextAlignment(.center)
            Spacer()
            box
            where_
            if let problem {
                Text(problem).font(Typography.footnote).foregroundStyle(Palette.failedText.color).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, Space.m)
        .padding(.bottom, Space.s)
        .frame(maxWidth: 744)
        .frame(maxWidth: .infinity)
        .screenBackground()
    }

    private var box: some View {
        VStack(alignment: .leading, spacing: 6) {
            TextField("Prompt", text: $draft.prompt, prompt: Text("Describe the task…"), axis: .vertical)
                .lineLimit(3...8)
                .font(Typography.body)
                .textFieldStyle(.plain)
                .focused($focused)
                .padding(.horizontal, 4)
                .padding(.top, 4)
            HStack(spacing: 0) {
                agentMenu
                modelMenu
                accessMenu
                Spacer(minLength: Space.s)
                startButton
            }
        }
        .padding(8)
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Palette.separator.color))
    }

    private var startButton: some View {
        let blocked = requestId != nil || providers == nil || problem != nil
        return Button(action: start) {
            Image(systemName: "arrow.up")
                .font(.system(size: 15, weight: .bold))
                .foregroundStyle((blocked ? Palette.text4 : Palette.onText).color)
                .frame(width: 34, height: 34)
                .background((blocked ? Palette.fill3 : Palette.text).color, in: Circle())
        }
        .buttonStyle(.plain)
        .disabled(blocked)
        .accessibilityLabel("Start")
    }

    private var agentMenu: some View {
        Menu {
            Picker("Agent", selection: Binding(get: { draft.provider }, set: { draft.pick(provider: $0) })) {
                ForEach(providers?.providers.filter(\.available) ?? [], id: \.id) { Text(SessionText.provider($0.id)).tag(ProviderId(rawValue: $0.id)) }
            }
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
        } label: {
            ComposerPill(text: SessionText.provider(draft.provider.rawValue)) { BrandMark(draft.provider.rawValue, size: 12) }
        }
        .menuOrder(.fixed)
    }

    private var modelMenu: some View {
        let models = NewSessionDraft.models(draft.provider)
        let efforts = info?.efforts ?? []
        let model = draft.model.map(Sidebar.modelName) ?? "Default model"
        return Menu {
            if !models.isEmpty {
                Picker("Model", selection: $draft.model) {
                    Text("Default").tag(String?.none)
                    ForEach(models, id: \.self) { Text(Sidebar.modelName($0)).tag(Optional($0)) }
                }
            }
            if !efforts.isEmpty {
                Picker("Effort", selection: $draft.effort) {
                    Text("Default Effort").tag(String?.none)
                    ForEach(efforts, id: \.self) { Text($0.capitalized).tag(Optional($0)) }
                }
                .pickerStyle(.menu)
            }
        } label: {
            ComposerPill(text: draft.effort.map { "\(model) · \($0.capitalized)" } ?? model) { EmptyView() }
        }
        .menuOrder(.fixed)
        .disabled(models.isEmpty && efforts.isEmpty)
    }

    private var accessMenu: some View {
        Menu {
            AccessChoices(choices: AccessPill.choices(max: maxAccess, launching: true), picked: draft.access.rawValue) { draft.access = Access(rawValue: $0) }
        } label: {
            ComposerPill(text: AccessPill.label(draft.access.rawValue)) { Image(systemName: "shield") }
        }
        .menuOrder(.fixed)
    }

    /// The desktop's footer: the project · a new Worktree or Local · the branch a worktree starts from.
    private var where_: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 0) {
                Menu {
                    Picker("Project", selection: $draft.project) {
                        ForEach(workspace.projects) { Text($0.name).tag($0.path) }
                    }
                } label: {
                    ComposerPill(text: project?.name ?? "Project") { ProjectTile(name: project?.name ?? "") }
                }
                .disabled(workspace.projects.count < 2)
                Text("·").font(Typography.footnote).foregroundStyle(Palette.text4.color)
                let place = places.first { $0.place == draft.place }
                Menu {
                    // Buttons, not a Picker: only a button's second text shows as the item's subtitle.
                    ForEach(places) { option in
                        Button { draft.place = option.place } label: {
                            Label(option.label, systemImage: option.place == draft.place ? "checkmark" : option.systemImage)
                            if let hint = option.hint { Text(hint) }
                        }
                    }
                } label: {
                    ComposerPill(text: place?.label ?? "Worktree") { Image(systemName: place?.systemImage ?? "arrow.triangle.branch") }
                }
                .menuOrder(.fixed)
                if draft.place == .newWorktree {
                    Button { worktreeOptions = true } label: {
                        ComposerPill(text: branchLabel, mono: true) { Image(systemName: branchIcon) }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
    }

    private var branchIcon: String {
        if case .pr = draft.source { "arrow.triangle.pull" } else { "arrow.triangle.branch" }
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
        if let providers = try? await workspace.loadProviders() {
            draft.access = NewSessionDraft.defaultAccess(max: providers.phoneMaxAccess)
            if !providers.providers.contains(where: { $0.id == draft.provider.rawValue && $0.available }),
               let first = providers.providers.first(where: \.available) {
                draft.pick(provider: ProviderId(rawValue: first.id))
            }
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
        let plan = draft.plan(taken: taken, localId: Sidebar.newLocalId(), localName: workspace.nextLocalName(project: draft.project))
        requestId = id
        Task {
            do {
                let made = try await workspace.launch(plan, requestId: id)
                Haptics.success()
                workspace.forget(creation: id)
                dismiss()
                openURL(DeepLink.agent(mac: mac, agentId: made.agentId).url)
            } catch {
                Haptics.warning()
            }
        }
    }

    static func main(of project: Project?) -> String? {
        project.map { $0.worktrees.first(where: \.isMain)?.path ?? $0.path }
    }
}

/// A composer's menu button: an icon, what it holds, and the up-down chevron of a choice.
@MainActor
private struct ComposerPill<Icon: View>: View {
    let text: String
    var mono = false
    @ViewBuilder let icon: Icon

    var body: some View {
        HStack(spacing: 5) {
            icon.font(.system(size: 11))
            Text(text).font(mono ? Typography.codeSmall : Typography.footnote).lineLimit(1)
            Image(systemName: "chevron.up.chevron.down").font(.system(size: 8, weight: .semibold)).foregroundStyle(Palette.text4.color)
        }
        .foregroundStyle(Palette.text2.color)
        .padding(.horizontal, 8)
        .frame(height: 28)
        .contentShape(Rectangle())
    }
}

/// A project's initials on a small tile, as the desktop's project chip.
@MainActor
private struct ProjectTile: View {
    let name: String

    var body: some View {
        RoundedRectangle(cornerRadius: 4, style: .continuous)
            .fill(Palette.fill3.color)
            .overlay(Text(Workspaces.initials(name)).font(.system(size: 8, weight: .bold)).foregroundStyle(Palette.text2.color))
            .frame(width: 16, height: 16)
    }
}
