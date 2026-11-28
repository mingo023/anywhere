import DesignSystem
import Stores
import SwiftUI
import Wire

/// Pushes the branch and opens its PR: its title, description, base and whether it's a draft.
@MainActor
struct CreatePullRequestSheet: View {
    let store: GitHubStore
    let model: TreePullRequest
    let newPr: NewPr
    let branches: (@MainActor () async throws -> BranchList)?
    /// Creating waits for the Mac.
    let offline: Bool

    @State private var title: String
    @State private var bodyText: String
    /// Nil opens it into the repository's default branch.
    @State private var base: String?
    @State private var error: String?
    @AppStorage("github.draft") private var draft = false
    @Environment(\.dismiss) private var dismiss

    init(store: GitHubStore, tree: String, newPr: NewPr, branches: (@MainActor () async throws -> BranchList)? = nil, offline: Bool = false) {
        self.store = store
        model = store.pullRequest(tree)
        self.newPr = newPr
        self.branches = branches
        self.offline = offline
        let filled = PullRequestText.fromCommits(newPr.commits)
        _title = State(initialValue: filled.title)
        _bodyText = State(initialValue: filled.body)
        _base = State(initialValue: newPr.base.map(PullRequestText.baseBranch))
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    label("Title", top: Space.l)
                    TextField("Title", text: $title)
                        .textFieldStyle(.plain)
                        .font(.system(size: 17))
                        .card()
                    label("Description", top: Space.xl)
                    TextField("What this changes, and why", text: $bodyText, axis: .vertical)
                        .textFieldStyle(.plain)
                        .font(.system(size: 17))
                        .foregroundStyle(Palette.textBody.color)
                        .lineLimit(3...16)
                        .frame(minHeight: 70, alignment: .topLeading)
                        .card()
                    options
                    Label {
                        Text("Pushes the branch first")
                    } icon: {
                        LucideIcon(.upload, size: 13)
                    }
                    .font(.system(size: 13))
                    .foregroundStyle(Palette.text3.color)
                    .padding(.horizontal, Inset.margin * 2)
                    .padding(.top, Space.s)
                    if let error {
                        Text(error).font(.system(size: 13)).foregroundStyle(Palette.failedText.color)
                            .padding(.horizontal, Inset.margin * 2).padding(.top, Space.m)
                    }
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .screenBackground()
            .safeAreaInset(edge: .bottom, spacing: 0) { createButton }
            .navigationTitle("New pull request")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { LucideIcon(.close, size: 16) }.accessibilityLabel("Close")
                }
            }
            .interactiveDismissDisabled(model.busy != nil)
        }
    }

    private func label(_ text: String, top: CGFloat) -> some View {
        Text(text).font(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.text2.color)
            .padding(.horizontal, Inset.margin * 2).padding(.top, top).padding(.bottom, Space.s)
    }

    private var options: some View {
        VStack(spacing: 0) {
            NavigationLink {
                BasePicker(branches: branches, mac: store.macName, selection: $base)
            } label: {
                HStack(spacing: Space.xs) {
                    Text("Base").font(.system(size: 17)).foregroundStyle(Palette.text.color)
                    Spacer(minLength: Space.s)
                    Text(base ?? "default").font(.system(size: 15, design: .monospaced)).foregroundStyle(Palette.text2.color).lineLimit(1)
                    LucideIcon(.unfold, size: 16).foregroundStyle(Palette.text3.color)
                }
                .frame(minHeight: 44)
                .padding(.leading, Inset.margin)
                .padding(.trailing, Space.m)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Toggle(isOn: $draft) { Text("Draft").font(.system(size: 17)) }
                .toggleStyle(.switch)
                .tint(Palette.text.color)
                .frame(minHeight: 44)
                .padding(.horizontal, Inset.margin)
                .rowDivider(true, leading: Inset.margin)
        }
        .insetGroup()
        .padding(.top, Space.xl)
    }

    private var createButton: some View {
        Button { create() } label: {
            Text(PullRequestText.createLabel(draft: draft, busy: model.busy)).font(.system(size: 17, weight: .semibold)).frame(maxWidth: .infinity)
        }
        .buttonStyle(PillButtonStyle(.primary, height: 52))
        .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty || model.busy != nil || offline)
        .padding(.horizontal, 20)
        .padding(.bottom, Space.s)
    }

    private func create() {
        error = nil
        Task {
            do {
                _ = try await store.create(model.tree, title: title.trimmingCharacters(in: .whitespaces), body: bodyText, draft: draft, base: base)
                Haptics.success()
                dismiss()
            } catch {
                Haptics.warning()
                self.error = store.describe(error)
            }
        }
    }
}

extension View {
    /// A field on its own surface, 16 pt in from the edges.
    fileprivate func card() -> some View {
        padding(.horizontal, Inset.margin)
            .padding(.vertical, 13)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: Inset.radius, style: .continuous))
            .padding(.horizontal, Inset.margin)
    }
}
