import SwiftUI

/// A branch in a picker: its name, what sets it apart ("On origin"), and a check on the one picked.
@MainActor
public struct BranchRow: View {
    let label: String
    let detail: String?
    let picked: Bool
    let pick: () -> Void

    public init(_ label: String, detail: String?, picked: Bool, pick: @escaping () -> Void) {
        self.label = label
        self.detail = detail
        self.picked = picked
        self.pick = pick
    }

    public var body: some View {
        Button(action: pick) {
            HStack {
                VStack(alignment: .leading, spacing: 0) {
                    Text(label).font(Typography.code).foregroundStyle(Palette.text.color)
                    if let detail { Text(detail).font(Typography.caption).foregroundStyle(Palette.text3.color) }
                }
                Spacer()
                if picked { Image(systemName: "checkmark").foregroundStyle(Palette.accent.color) }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
