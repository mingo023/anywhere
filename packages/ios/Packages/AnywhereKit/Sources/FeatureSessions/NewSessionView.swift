import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The new session sheet: which agent, how, on which account, where, and the first prompt. Starting shows pocketd's
/// steps, then opens the chat.
@MainActor
struct NewSessionView: View {
    let host: HostModel
    let mac: String
    @State private var draft: NewSessionDraft
    @State private var requestId: String?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    init(host: HostModel, mac: String, project: String?, place: NewSessionDraft.Place?) {
        self.host = host
        self.mac = mac
        let path = project ?? host.workspace.projects.first?.path ?? ""
        var draft = NewSessionDraft(project: path, place: place ?? .tree(Self.main(of: host.workspace.project(path)) ?? path))
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

    var body: some View {
        Form {
            if let requestId {
                CreationProgress(creation: workspace.creations[requestId]) {
                    if let creation = workspace.creations[requestId] { draft.resume(after: creation) }
                    workspace.forget(creation: requestId)
                    self.requestId = nil
                }
            } else {
                fields
            }
        }
        .formStyle(.grouped)
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
            ToolbarItem(placement: .confirmationAction) {
                Button("Start", action: start).disabled(requestId != nil || providers == nil || problem != nil)
            }
        }
        .task { await load() }
        .onChange(of: draft.provider) { _, provider in Task { try? await workspace.loadAccounts(provider: provider.rawValue) } }
        .onChange(of: draft.project) { _, _ in
            draft.place = .tree(mainTree)
            Task { await loadBranches() }
        }
    }

    @ViewBuilder
    private var fields: some View {
        if workspace.projects.count > 1 {
            Picker("Project", selection: $draft.project) {
                ForEach(workspace.projects) { Text($0.name).tag($0.path) }
            }
        }
        if let project {
            Section("Where") {
                Picker("Where", selection: $draft.place) {
                    ForEach(Places.options(project: project, locals: workspace.locals, names: workspace.names, canManage: workspace.canManage,
                                           canSpawn: canSpawn)) { option in
                        Label(option.label, systemImage: option.systemImage).tag(option.place)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
                if draft.place == .newWorktree {
                    WorktreeFields(draft: $draft, branches: workspace.branches[mainTree], canChooseSource: workspace.canManage, taken: taken)
                }
            }
        }
        Section {
            ChipRow(options: (providers?.providers.filter(\.available) ?? []).map { (ProviderId(rawValue: $0.id), SessionText.provider($0.id)) },
                    selection: Binding(get: { draft.provider }, set: { draft.pick(provider: $0) }))
            let models = NewSessionDraft.models(draft.provider)
            if !models.isEmpty {
                ChipRow(options: [(String?.none, "Default")] + models.map { (Optional($0), Sidebar.modelName($0)) }, selection: $draft.model)
            }
            if let efforts = info?.efforts, !efforts.isEmpty {
                ChipRow(options: [(String?.none, "Default effort")] + efforts.map { (Optional($0), $0.capitalized) }, selection: $draft.effort)
            }
            if info?.plan == true { Toggle("Plan first", isOn: $draft.plan) }
            Toggle("Chat only", isOn: $draft.chat)
        } header: {
            Text("Agent")
        } footer: {
            Text(draft.chat ? "Runs without a terminal: you drive it from the chat." : "Runs in a terminal you can open too.")
        }
        Section {
            ChipRow(options: NewSessionDraft.accesses(max: maxAccess).map { ($0, NewSessionDraft.accessLabel($0)) }, selection: $draft.access)
        } header: {
            Text("Access")
        } footer: {
            Text(NewSessionDraft.accessHint(draft.access))
        }
        if let accounts = workspace.accounts[draft.provider.rawValue]?.filter(\.signedIn), !accounts.isEmpty {
            Section("Account") {
                Picker("Account", selection: $draft.account) {
                    // pocketd picks the project's account, else the Mac's default; the built-in login is listed as its own row.
                    Text("Default").tag(String?.none)
                    ForEach(accounts) { Text(SessionText.account($0)).tag(Optional($0.id)) }
                }
            }
        }
        Section {
            TextField("Prompt", text: $draft.prompt, prompt: Text("What should it do?"), axis: .vertical).lineLimit(3...10)
        } header: {
            Text("Prompt")
        } footer: {
            if let problem { Text(problem).foregroundStyle(Palette.failedText.color) }
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
