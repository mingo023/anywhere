import DesignSystem
import SwiftUI

/// Find in a file: the query, where the current match is, previous and next, and Done.
@MainActor
struct FindBar: View {
    @Binding var query: String
    let position: String
    let hasMatches: Bool
    let step: (Int) -> Void
    let done: () -> Void
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: Space.xs) {
            HStack(spacing: Space.xs) {
                Image(systemName: "magnifyingglass").foregroundStyle(Palette.text3.color)
                TextField("Find", text: $query)
                    .focused($focused)
                    .autocorrectionDisabled()
                    .submitLabel(.search)
                    .onSubmit { step(1) }
                #if os(iOS)
                    .textInputAutocapitalization(.never)
                #endif
                Text(position)
                    .font(Typography.footnote.monospacedDigit())
                    .foregroundStyle(Palette.text3.color)
                    .lineLimit(1)
                    .fixedSize()
            }
            .padding(.horizontal, Space.s)
            .frame(minHeight: 36)
            .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small))
            Button("Previous Match", systemImage: "chevron.up") { step(-1) }
                .labelStyle(.iconOnly)
                .frame(minWidth: 44, minHeight: 44)
                .disabled(!hasMatches)
            Button("Next Match", systemImage: "chevron.down") { step(1) }
                .labelStyle(.iconOnly)
                .frame(minWidth: 44, minHeight: 44)
                .disabled(!hasMatches)
            Button("Done", action: done)
                .frame(minHeight: 44)
        }
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.xxs)
        .background(.bar)
        .overlay(alignment: .bottom) { Divider() }
        .onAppear { focused = true }
    }
}
