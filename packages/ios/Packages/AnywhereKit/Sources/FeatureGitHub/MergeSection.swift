import DesignSystem
import Stores
import SwiftUI
import Wire

/// An open PR's merge: what holds it back, Ready for review or Merge with the repository's last method, and whether
/// the worktree goes once merged.
@MainActor
struct MergeSection: View {
    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    let offline: Bool
    let cleanup: WorktreeCleanup?
    @Binding var deleteAfter: Bool
    let merged: () -> Void

    @AppStorage private var method: MergeMethod
    @State private var merging = false

    init(
        store: GitHubStore, model: TreePullRequest, pr: PullRequest, offline: Bool, cleanup: WorktreeCleanup?, deleteAfter: Binding<Bool>,
        merged: @escaping () -> Void
    ) {
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
        Section("Merge") {
            Label {
                Text(PullRequestText.gate(pr.gate, base: pr.base)).foregroundStyle(Palette.text.color)
            } icon: {
                let tone: PrTone = pr.gate == .ready ? .success : pr.gate == .conflicts || pr.gate == .checksFailed ? .failed : .waiting
                Image(systemName: pr.gate == .ready ? "checkmark.seal.fill" : "exclamationmark.triangle.fill").foregroundStyle(tone.text.color)
            }
            if let line = PullRequestText.mergeable(pr) {
                Label {
                    Text(line).foregroundStyle(Palette.text2.color)
                } icon: {
                    let clean = pr.mergeable == "MERGEABLE"
                    Image(systemName: clean ? "checkmark.circle" : "arrow.triangle.branch")
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
                .buttonStyle(.borderedProminent)
                .disabled(model.busy != nil || offline)
            } else {
                mergeButtons
                if let cleanup {
                    Toggle(isOn: $deleteAfter) {
                        VStack(alignment: .leading, spacing: Space.xxs) {
                            Text("Delete the worktree after merging").foregroundStyle(Palette.text.color)
                            Text(cleanup.closes).font(Typography.footnote).foregroundStyle(Palette.text3.color)
                        }
                    }
                }
            }
        }
    }

    private var mergeButtons: some View {
        HStack(spacing: Space.s) {
            Button {
                merging = true
            } label: {
                Label(model.busy ?? method.label, systemImage: "arrow.triangle.merge").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .tint(Palette.success.color)
            Menu {
                Picker("Merge method", selection: $method) {
                    ForEach(MergeMethod.allCases, id: \.self) { Text($0.label).tag($0) }
                }
            } label: {
                Image(systemName: "chevron.down").hitTarget()
            }
            .accessibilityLabel("Other methods…")
            .buttonStyle(.bordered)
            .fixedSize()
        }
        .disabled(pr.gate != .ready || model.busy != nil || offline)
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
