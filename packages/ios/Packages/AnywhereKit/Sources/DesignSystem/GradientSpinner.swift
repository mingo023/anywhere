import SwiftUI

/// The desktop's working indicator (`gradient_spinner`): a 3×3 grid of dots tinted along the diagonal, a pulse of light
/// sweeping over them from the top-left. Every spinner reads the same clock, so those on screen pulse together.
@MainActor
public struct GradientSpinner: View {
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 14) {
        self.size = size
    }

    nonisolated static let period = 0.8
    nonisolated static let diagonals: [UInt32] = [0x8fb8ffff, 0xa9aafaff, 0xc39cf5ff, 0xe19db7ff, 0xff9f7aff]
    nonisolated static let dim = 0.1

    /// A dot's opacity `t` through its own pulse: full at the start, down to dim by 45%, resting, back to full over the last 8%.
    nonisolated static func opacity(_ t: Double) -> Double {
        let t = t - t.rounded(.down)
        if t < 0.45 { return 1 + (dim - 1) * t / 0.45 }
        if t < 0.92 { return dim }
        return dim + (1 - dim) * (t - 0.92) / 0.08
    }

    /// How far behind in its pulse the dot at `row`, `col` runs, as a fraction of a period.
    nonisolated static func delay(row: Int, col: Int) -> Double {
        Double(row + col) / 8
    }

    public var body: some View {
        // Capped as the desktop's is: uncapped, a repeating animation redraws every display frame.
        TimelineView(.animation(minimumInterval: 1.0 / 20, paused: reduceMotion)) { context in
            let t = reduceMotion ? 0 : context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: Self.period) / Self.period
            Canvas { canvas, box in
                for row in 0..<3 {
                    for col in 0..<3 {
                        let (r, g, b, _) = Token.components(Self.diagonals[row + col])
                        canvas.fill(Path(ellipseIn: Self.dot(row: row, col: col, in: box)), with: .color(Color(red: r, green: g, blue: b, opacity: Self.opacity(t - Self.delay(row: row, col: col)))))
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    nonisolated static func dot(row: Int, col: Int, in box: CGSize) -> CGRect {
        let dot = box.width / 4
        return CGRect(x: CGFloat(col) * dot * 1.5, y: CGFloat(row) * dot * 1.5, width: dot, height: dot)
    }
}

/// The spinner's grid at rest, as the desktop's `dot_matrix`: `lit` dots in `color`, the rest dim. `lit` holds one bit per
/// dot from the top-left, row by row.
@MainActor
public struct DotMatrix: View {
    public nonisolated static let full: UInt16 = 0b111_111_111
    public nonisolated static let cross: UInt16 = 0b101_010_101

    let lit: UInt16
    let color: Token
    let size: CGFloat

    public init(_ lit: UInt16, color: Token, size: CGFloat) {
        self.lit = lit
        self.color = color
        self.size = size
    }

    public var body: some View {
        Canvas { canvas, box in
            for row in 0..<3 {
                for col in 0..<3 {
                    let shade = color.color.opacity(Self.isLit(lit, row: row, col: col) ? 1 : GradientSpinner.dim)
                    canvas.fill(Path(ellipseIn: GradientSpinner.dot(row: row, col: col, in: box)), with: .color(shade))
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    nonisolated static func isLit(_ lit: UInt16, row: Int, col: Int) -> Bool {
        lit >> (8 - row * 3 - col) & 1 == 1
    }
}
