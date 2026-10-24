import SwiftUI

/// Light sweeping across a label while its work runs, redrawn at most 20 times a second.
public struct Shimmer: ViewModifier {
    let active: Bool

    public init(active: Bool = true) { self.active = active }

    public func body(content: Content) -> some View {
        if active {
            content.overlay {
                TimelineView(.animation(minimumInterval: 1.0 / 20)) { context in
                    let phase = Self.phase(at: context.date)
                    LinearGradient(
                        stops: [
                            .init(color: .clear, location: phase - 0.2), .init(color: Palette.shimmer.color.opacity(0.9), location: phase),
                            .init(color: .clear, location: phase + 0.2),
                        ],
                        startPoint: .leading, endPoint: .trailing
                    )
                    .mask(content)
                    .blendMode(.plusLighter)
                }
                .allowsHitTesting(false)
            }
        } else {
            content
        }
    }

    /// Where the band's centre is, from just before the leading edge to just past the trailing one, every 1.6 s.
    static func phase(at date: Date) -> Double {
        let period = 1.6
        return -0.2 + 1.4 * date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period) / period
    }
}

extension View {
    public func shimmer(_ active: Bool = true) -> some View { modifier(Shimmer(active: active)) }
}
