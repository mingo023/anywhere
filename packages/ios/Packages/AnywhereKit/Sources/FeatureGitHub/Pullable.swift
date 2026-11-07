import SwiftUI

/// A full-screen state that pull-to-refresh reaches: a placeholder alone has no scroll view to pull.
@MainActor
struct Pullable<Content: View>: View {
    let refresh: @MainActor () async -> Void
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            content.containerRelativeFrame([.horizontal, .vertical])
        }
        .refreshable { await refresh() }
    }
}
