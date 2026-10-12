import SwiftUI

/// The provider and how the ask stands, either side of the notch.
struct Compact: View {
    let ask: Ask
    let stage: Stage
    /// The asks waiting, this one included.
    let count: Int
    let width: CGFloat
    let height: CGFloat

    var body: some View {
        HStack {
            HStack(spacing: 7) {
                ProviderIcon(ask: ask, size: 14)
                Text(ask.name)
            }
            Spacer(minLength: 0)
            HStack(spacing: 7) {
                mark
                Text(label)
            }
            .foregroundColor(tint)
        }
        .font(.system(size: 12, weight: .medium))
        .foregroundColor(.ink2)
        .padding(.leading, 13)
        .padding(.trailing, 14)
        .frame(width: width, height: height)
    }

    @ViewBuilder private var mark: some View {
        switch stage {
        case .done(.deny): Image(systemName: "xmark").font(.system(size: 10, weight: .bold))
        case .done: Image(systemName: "checkmark").font(.system(size: 10, weight: .bold))
        default: Circle().fill(Color.waiting).frame(width: 7, height: 7)
        }
    }

    private var label: String {
        switch stage {
        case .done(.allow): "Allowed"
        case .done(.deny): "Stopped"
        case .done(.reply): "Sent"
        default: count > 1 ? "Needs you · \(count)" : "Needs you"
        }
    }

    private var tint: Color {
        switch stage {
        case .done(.deny): .stopped
        case .done: .allowed
        default: .ink2
        }
    }
}
