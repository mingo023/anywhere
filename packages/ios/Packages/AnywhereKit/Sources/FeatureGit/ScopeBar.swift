import DesignSystem
import SwiftUI

/// Uncommitted or Compare, and the branch Compare is against.
@MainActor
struct ScopeBar: View {
    @Binding var comparing: Bool
    let against: String

    var body: some View {
        Segments(options: [(false, "Uncommitted"), (true, "vs \(against)")], selection: $comparing)
            .padding(.top, Space.s)
    }
}
