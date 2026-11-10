import DesignSystem
import Stores
import SwiftUI
import Wire

/// An open PR's merge: what holds it back, Ready for review or Resolve, and whether the worktree goes once merged. The
/// merge itself is the bar under the card.
@MainActor
struct MergeSection: View {
    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    let offline: Bool
    let cleanup: WorktreeCleanup?
    @Binding var deleteAfter: Bool
    /// Resolve with the agent's title, standing in for Merge while the PR conflicts; nil when it can't be sent.
    let resolveTitle: String?
    /// What the merge row says while the agent resolves the conflict.
    let resolving: String?
    let resolve: () -> Void

    init(
        store: GitHubStore, model: TreePullRequest, pr: PullRequest, offline: Bool, cleanup: WorktreeCleanup?, deleteAfter: Binding<Bool>,
        resolveTitle: String? = nil, resolving: String? = nil, resolve: @escaping () -> Void = {}
    ) {
        self.store = store
        self.model = model
        self.pr = pr
        self.offline = offline
        self.cleanup = cleanup
        _deleteAfter = deleteAfter
        self.resolveTitle = resolveTitle
        self.resolving = resolving
        self.resolve = resolve
    }

    var body: some View {
        Section("Merge") {
            Label {
                Text(resolving ?? PullRequestText.gate(pr.gate, base: pr.base)).foregroundStyle(Palette.text.color)
            } icon: {
                let tone: PrTone = pr.gate == .ready ? .success : pr.gate == .conflicts || pr.gate == .checksFailed ? .failed : .waiting
                Image(systemName: pr.gate == .ready ? "checkmark.seal.fill" : "exclamationmark.triangle.fill").foregroundStyle(tone.text.color)
            }
            if let line = PullRequestText.mergeable(pr) {
                Label {
                    Text(line).foregroundStyle(Palette.text2.color)
                } icon: {
                    let clean = pr.mergeable == "MERGEABLE"
                    Image.named(clean ? "checkmark.circle" : "branch", size: 16)
                        .foregroundStyle((clean ? PrTone.success : pr.mergeable == "CONFLICTING" ? .failed : .muted).text.color)
                }
            }
            if pr.draft {
                Button {
                    Haptics.tap()
                    Task { await store.markReady(model.tree) }
                } label: {
                    Text(model.busy ?? "Ready for review").frame(maxWidth: .infinity)
                }
                .buttonStyle(CapsuleButtonStyle())
                .disabled(model.busy != nil || offline)
            } else {
                if pr.gate == .conflicts, let resolveTitle {
                    Button {
                        Haptics.tap()
                        resolve()
                    } label: {
                        Text(resolveTitle).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(CapsuleButtonStyle())
                    .disabled(resolving != nil || offline)
                }
                if let cleanup {
                    Toggle(isOn: $deleteAfter) {
                        VStack(alignment: .leading, spacing: Space.xxs) {
                            Text("Delete the worktree after merging").foregroundStyle(Palette.text.color)
                            Text(cleanup.closes).font(Typography.footnote).foregroundStyle(Palette.text3.color)
                        }
                    }
                    .tint(Palette.text.color)
                }
            }
        }
    }
}

/// The method to merge with, what holds it back, and Merge, kept in reach at the bottom of the card.
@MainActor
struct MergeBar: View {
    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    let offline: Bool
    let cleanup: WorktreeCleanup?
    @Binding var deleteAfter: Bool
    let merged: () -> Void

    @AppStorage private var method: MergeMethod
    @State private var merging = false

    init(store: GitHubStore, model: TreePullRequest, pr: PullRequest, offline: Bool, cleanup: WorktreeCleanup?, deleteAfter: Binding<Bool>, merged: @escaping () -> Void) {
        self.store = store
        self.model = model
        self.pr = pr
        self.offline = offline
        self.cleanup = cleanup
        _deleteAfter = deleteAfter
        self.merged = merged
        _method = AppStorage(wrappedValue: .squash, "github.mergeMethod.\(PullRequestText.repo(pr.url) ?? "")")
    }

    var body: some View {
        HStack(spacing: Space.m) {
            VStack(alignment: .leading, spacing: 1) {
                Menu {
                    Picker("Merge method", selection: $method) {
                        ForEach(MergeMethod.allCases, id: \.self) { Text($0.label).tag($0) }
                    }
                } label: {
                    HStack(spacing: Space.xs) {
                        Text(method.label).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                        Image(systemName: "chevron.down").font(Typography.caption2.weight(.semibold)).foregroundStyle(Palette.text3.color)
                    }
                }
                .accessibilityLabel("Merge method")
                Text(model.busy ?? PullRequestText.gate(pr.gate, base: pr.base)).font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(1)
            }
            Spacer(minLength: Space.s)
            Button("Merge") { merging = true }
                .buttonStyle(CapsuleButtonStyle())
                .disabled(pr.gate != .ready || model.busy != nil || offline)
        }
        .padding(.leading, Space.l)
        .padding(.trailing, Space.s)
        .padding(.vertical, Space.s)
        .glass(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .padding(.horizontal, Space.l)
        .padding(.bottom, Space.xs)
        .confirmationDialog("Merge #\(pr.number) into \(pr.base)?", isPresented: $merging, titleVisibility: .visible) {
            let chosen = method
            Button(chosen.label) {
                Task {
                    await store.merge(model.tree, method: chosen)
                    if model.pr?.state == "MERGED" {
                        Haptics.success()
                        merged()
                    } else {
                        Haptics.warning()
                    }
                }
            }
        } message: {
            Text(deleteAfter && cleanup != nil ? "The branch stays; then the worktree is deleted." : "The branch stays.")
        }
    }
}
