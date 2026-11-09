import SwiftUI

/// The desktop's field behind a prompt page (`pocket/src/field.rs`): a dot grid hiding a contour map, lit as ASCII under
/// a lens. A phone has no cursor, so the lens drifts around `rest` on its own and follows a finger on the empty space;
/// with Reduce Motion it stays at `rest`.
@MainActor
public struct AsciiField: View {
    /// What the field fades around, in its own coordinates.
    let content: [CGRect]
    let rest: CGPoint
    @State private var lens = FieldLens()
    @State private var stepped: Date?
    @State private var dragging = false
    @State private var map = FieldMap()
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(content: [CGRect], rest: CGPoint) {
        self.content = content
        self.rest = rest
    }

    public var body: some View {
        // Capped while it drifts: the field redraws every cell, and a slow lens lights no new ones at 60 fps.
        TimelineView(.animation(minimumInterval: dragging ? nil : 1.0 / 20, paused: reduceMotion && !lens.moving)) { timeline in
            Canvas { canvas, size in draw(&canvas, size) }
                .onChange(of: timeline.date) { _, now in step(now) }
        }
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { drag in
                    dragging = true
                    aim(drag.location)
                }
                .onEnded { _ in
                    dragging = false
                    aim(rest)
                }
        )
        .onAppear { aim(rest) }
        .onChange(of: rest) { _, rest in if !dragging { aim(rest) } }
        .accessibilityHidden(true)
    }

    private func aim(_ at: CGPoint) {
        lens.aim(at)
        if reduceMotion { lens.settle() } else { stepped = nil }
    }

    private func step(_ now: Date) {
        let dt = stepped.map { min(now.timeIntervalSince($0), 0.064) } ?? 0
        stepped = now
        if !dragging && !reduceMotion { lens.wander(dt, around: rest) }
        lens.step(dt)
    }

    private func draw(_ canvas: inout GraphicsContext, _ size: CGSize) {
        guard !content.isEmpty else { return }
        let dark = scheme == .dark
        let (cols, rows) = (Int((size.width / Field.cellW).rounded(.up)), Int((size.height / Field.cellH).rounded(.up)))
        let grid = map.sized(cols, rows, around: content)
        let faint = Token.components(Palette.text6.rgba(dark: dark))
        let soft = Token.components(Palette.text5.rgba(dark: dark))
        let strong = Token.components(Palette.text2.rgba(dark: dark))
        // Thousands of cells a frame: dots batch into one path per alpha step, and glyphs share resolved text per shade.
        let steps = 8
        var dots = [Path](repeating: Path(), count: steps)
        var glyphs: [Int: GraphicsContext.ResolvedText] = [:]
        for row in 0..<rows {
            for col in 0..<cols {
                let p = Field.center(col, row)
                let cell = row * cols + col
                switch Field.mark(level: grid.levels[cell], glow: lens.glow(p), mask: grid.masks[cell]) {
                case .hidden:
                    break
                case .dot(let alpha):
                    dots[min(Int(alpha * Double(steps)), steps - 1)].addEllipse(in: CGRect(x: p.x - 1, y: p.y - 1, width: 2, height: 2))
                case .glyph(let index, let shade, let alpha):
                    let tone = min(Int(shade * Double(steps)), steps - 1)
                    let key = index * steps + tone
                    let text = glyphs[key] ?? canvas.resolve(
                        Text(Field.ramp[index]).font(.system(size: 12, design: .monospaced)).foregroundColor(Self.mix(soft, strong, Double(tone) / Double(steps - 1))))
                    glyphs[key] = text
                    var layer = canvas
                    layer.opacity = alpha
                    layer.draw(text, at: p, anchor: .center)
                }
            }
        }
        let (r, g, b, _) = faint
        for (i, path) in dots.enumerated() where !path.isEmpty {
            canvas.fill(path, with: .color(Color(red: r, green: g, blue: b, opacity: (Double(i) + 0.5) / Double(steps))))
        }
    }

    private static func mix(_ a: (Double, Double, Double, Double), _ b: (Double, Double, Double, Double), _ t: Double) -> Color {
        Color(red: a.0 + (b.0 - a.0) * t, green: a.1 + (b.1 - a.1) * t, blue: a.2 + (b.2 - a.2) * t)
    }
}

/// The lens: where it is and how far it has faded in, and where each of those is heading.
struct FieldLens: Equatable {
    static let radius: CGFloat = 170

    var x: CGFloat = 0
    var y: CGFloat = 0
    var lit: CGFloat = 0
    var aimX: CGFloat = 0
    var aimY: CGFloat = 0
    var aimLit: CGFloat = 0
    /// Seconds spent drifting, so a finger pauses the path rather than skipping it.
    var wandered: Double = 0
    /// Still easing, so the field needs frames.
    var moving = false

    mutating func aim(_ at: CGPoint) {
        (aimX, aimY, aimLit) = (at.x, at.y, 1)
        // An unlit lens appears where it's aimed rather than sliding in from where it faded out.
        if lit < 0.01 { (x, y) = (at.x, at.y) }
        moving = true
    }

    mutating func wander(_ dt: Double, around rest: CGPoint) {
        wandered += dt
        let at = Field.wander(wandered, around: rest)
        (aimX, aimY, aimLit) = (at.x, at.y, 1)
    }

    mutating func settle() {
        (x, y, lit) = (aimX, aimY, aimLit)
        moving = false
    }

    mutating func step(_ dt: Double) {
        let (follow, fade) = (1 - exp(-23 * dt), 1 - exp(-13 * dt))
        x += (aimX - x) * follow
        y += (aimY - y) * follow
        lit += (aimLit - lit) * fade
        moving = abs(aimX - x) + abs(aimY - y) > 0.4 || abs(aimLit - lit) > 0.004
    }

    /// How brightly the lens lights a point: 1 at its centre, 0 from `radius` out.
    func glow(_ p: CGPoint) -> Double {
        let d = hypot(p.x - x, p.y - y)
        return lit < 0.01 || d >= Self.radius ? 0 : Field.smooth(1 - d / Self.radius) * lit
    }
}

/// Each cell's height on the hidden contour map and how far it fades around the content, kept while the lens moves.
@MainActor
final class FieldMap {
    private var cols = 0
    private var content: [CGRect] = []
    private var levels: [Double] = []
    private var masks: [Double] = []

    func sized(_ cols: Int, _ rows: Int, around content: [CGRect]) -> (levels: [Double], masks: [Double]) {
        let resized = self.cols != cols || levels.count != cols * rows
        if resized {
            self.cols = cols
            levels = Self.cells(cols, rows) { c, r in Field.level(c, r) }
        }
        if resized || self.content != content {
            self.content = content
            masks = Self.cells(cols, rows) { c, r in
                let p = Field.center(c, r)
                var nearest = CGFloat.infinity
                for rect in content { nearest = min(nearest, Field.outside(rect, p)) }
                return Field.mask(nearest)
            }
        }
        return (levels, masks)
    }

    private static func cells(_ cols: Int, _ rows: Int, _ value: (Int, Int) -> Double) -> [Double] {
        (0..<rows).flatMap { r in (0..<cols).map { c in value(c, r) } }
    }
}

/// The field's rules, over plain numbers.
enum Field {
    static let cellW: CGFloat = 10
    static let cellH: CGFloat = 18
    static let ramp = ["·", ":", "-", "=", "+", "*", "#"]
    /// How far the field stays clear of the content, then fades in; tighter than the desktop's 24 and 110 on a phone's width.
    static let margin: CGFloat = 12
    static let fade: CGFloat = 64

    /// Where an idle lens is `t` seconds in: leaving `rest` and crossing the field. `rest` is the field's horizontal middle;
    /// the two periods don't line up, so the path doesn't visibly repeat.
    static func wander(_ t: Double, around rest: CGPoint) -> CGPoint {
        CGPoint(x: rest.x + rest.x * 0.8 * sin(2 * .pi * t / 17), y: rest.y + 36 * sin(2 * .pi * t / 11))
    }

    static func center(_ col: Int, _ row: Int) -> CGPoint {
        CGPoint(x: CGFloat(col) * cellW + cellW / 2, y: CGFloat(row) * cellH + cellH / 2)
    }

    enum Mark: Equatable {
        case hidden
        case dot(alpha: Double)
        /// `shade` runs from the soft colour (0) to the strong one (1).
        case glyph(index: Int, shade: Double, alpha: Double)
    }

    /// The mark for a cell at map `level`, lit `glow` by the lens, and `mask` from 0 at the content to 1 clear of it.
    static func mark(level: Double, glow: Double, mask: Double) -> Mark {
        if mask < 0.02 { return .hidden }
        let lit = glow * mask
        let strength = level * lit
        if strength < 0.06 { return .dot(alpha: mask * (0.35 + level * 0.65)) }
        return .glyph(index: min(Int(strength * Double(ramp.count)), ramp.count - 1), shade: min(strength * 1.3, 1), alpha: min(0.25 + lit, 1))
    }

    static func mask(_ distance: CGFloat) -> Double {
        smooth(max(0, min(1, (distance - margin) / fade)))
    }

    /// How far `p` lies outside `rect`; 0 inside.
    static func outside(_ rect: CGRect, _ p: CGPoint) -> CGFloat {
        hypot(max(rect.minX - p.x, p.x - rect.maxX, 0), max(rect.minY - p.y, p.y - rect.maxY, 0))
    }

    /// Near 1 on a contour line, rising gently with height in between.
    static func level(_ col: Int, _ row: Int) -> Double {
        let v = fbm(Double(col) * 0.035, Double(row) * 0.065) * 7
        let contour = 1 - min(abs(v - v.rounded()) * 4.5, 1)
        return max(0, min(1, contour * 0.8 + v / 7 * 0.35))
    }

    static func smooth(_ t: Double) -> Double { t * t * (3 - 2 * t) }

    static func fbm(_ x: Double, _ y: Double) -> Double {
        noise(x, y) * 0.55 + noise(x * 2.1, y * 2.1) * 0.3 + noise(x * 4.3, y * 4.3) * 0.15
    }

    /// Smoothly interpolated value noise in 0..1.
    static func noise(_ x: Double, _ y: Double) -> Double {
        let (fx, fy) = (x.rounded(.down), y.rounded(.down))
        let (xi, yi) = (Int32(fx), Int32(fy))
        let (u, v) = (smooth(x - fx), smooth(y - fy))
        let (a, b, c, d) = (hash(xi, yi), hash(xi &+ 1, yi), hash(xi, yi &+ 1), hash(xi &+ 1, yi &+ 1))
        return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
    }

    static func hash(_ x: Int32, _ y: Int32) -> Double {
        var h = UInt32(bitPattern: x &* 374_761_393 &+ y &* 668_265_263)
        h = (h ^ (h >> 13)) &* 1_274_126_177
        return Double(h ^ (h >> 16)) / 4_294_967_296
    }
}
