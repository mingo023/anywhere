import SwiftUI

/// The desktop's working indicator (`gradient_spinner`): a 3×3 grid of dots tinted by row, a pulse of light passing over
/// them. Every spinner reads the same clock, so those on screen pulse together.
@MainActor
public struct GradientSpinner: View {
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 14) {
        self.size = size
    }

    nonisolated static let period = 0.75
    nonisolated static let rows: [UInt32] = [0xb6d3efff, 0xedb185ff, 0xf888a0ff]
    nonisolated static let dim = 0.1

    /// A dot's opacity `t` through its own pulse: full at the start, down to dim by 45%, resting, back to full over the last 8%.
    nonisolated static func opacity(_ t: Double) -> Double {
        let t = t - t.rounded(.down)
        if t < 0.45 { return 1 + (dim - 1) * t / 0.45 }
        if t < 0.92 { return dim }
        return dim + (1 - dim) * (t - 0.92) / 0.08
    }

    /// How far ahead in its pulse the dot at `row`, `col` runs, as a fraction of a period.
    nonisolated static func lead(row: Int, col: Int) -> Double {
        (2 - Double(row) + abs(Double(col) - 1)) / 4
    }

    public var body: some View {
        // Capped as the desktop's is: uncapped, a repeating animation redraws every display frame.
        TimelineView(.animation(minimumInterval: 1.0 / 20, paused: reduceMotion)) { context in
            let t = reduceMotion ? 0 : context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: Self.period) / Self.period
            Canvas { canvas, box in
                let dot = box.width / 4
                let step = dot * 1.5
                for row in 0..<3 {
                    let (r, g, b, _) = Token.components(Self.rows[row])
                    for col in 0..<3 {
                        let rect = CGRect(x: CGFloat(col) * step, y: CGFloat(row) * step, width: dot, height: dot)
                        canvas.fill(Path(ellipseIn: rect), with: .color(Color(red: r, green: g, blue: b, opacity: Self.opacity(t + Self.lead(row: row, col: col)))))
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
