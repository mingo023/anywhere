import SwiftUI

/// The desktop's mascot (`mascot-*.svg`, drawn in `pocket/src/terminals/updating.rs`): body, sheen, antenna light and
/// eyes over a soft glow, in the app icon's colours in both appearances. Waiting, rings rise off its antenna twice a
/// beat and it blinks before the next; stuck, its light goes out and it looks down; happy, its eyes smile.
@MainActor
public struct Mascot: View {
    public enum Mood: Hashable, Sendable {
        case waiting, stuck, happy
    }

    let mood: Mood
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(_ mood: Mood = .waiting, size: CGFloat = 96) {
        self.mood = mood
        self.size = size
    }

    public var body: some View {
        let animates = mood == .waiting && !reduceMotion
        // Capped as the desktop's is: uncapped, a repeating animation redraws every display frame.
        TimelineView(.animation(minimumInterval: 1.0 / 20, paused: !animates)) { context in
            drawn(animates ? MascotLook.waiting(context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: MascotLook.beat) / MascotLook.beat) : MascotLook.of(mood))
        }
        .frame(width: size, height: size * MascotLook.height / 200)
        .background {
            RadialGradient(colors: [MascotLook.glow.color.opacity(0.4), .clear], center: .center, startRadius: 0, endRadius: size * 0.6)
                .frame(width: size * 1.2, height: size * 1.2)
                .offset(y: size * 0.13)
        }
        .accessibilityHidden(true)
    }

    private func drawn(_ look: MascotLook) -> some View {
        Canvas { canvas, box in MascotLook.draw(look, in: &canvas, size: box.width) }
    }
}

/// One frame of the mascot, as the desktop's `Look`.
struct MascotLook: Equatable {
    /// How open its eyes are; 1 is open.
    var lid = 1.0
    var rings: [Ring] = [Ring(), Ring()]
    /// Where its eyes look, in the SVG's units.
    var eyes = CGSize(width: 5, height: 1)
    var happy = false
    var lit = true

    struct Ring: Equatable {
        var scale = 0.0
        var opacity = 0.0
    }

    nonisolated static let beat = 4.4
    /// The SVG's 200 units tall, with room above for the rings rising off its antenna.
    nonisolated static let height: CGFloat = 220
    nonisolated static let still = MascotLook()
    nonisolated static let body = Token.fixed(0x1a1a1eff)
    nonisolated static let eyesColor = Token.fixed(0xf4f2eeff)
    nonisolated static let lightOn = Token.fixed(0x8ee6ccff)
    nonisolated static let lightOff = Token.fixed(0x24302cff)
    nonisolated static let glow = Token.fixed(0x7a68bfff)

    nonisolated static func of(_ mood: Mascot.Mood) -> MascotLook {
        switch mood {
        case .waiting: still
        case .stuck: MascotLook(eyes: CGSize(width: 4, height: 6), lit: false)
        case .happy: MascotLook(eyes: .zero, happy: true)
        }
    }

    /// The mascot `t` of the way through a beat while it waits: two pulses a beat, each ring moving through the first half
    /// of its pulse, the second a tenth behind; then a blink at the end.
    nonisolated static func waiting(_ t: Double) -> MascotLook {
        func ring(_ delay: Double) -> Ring {
            let pulse = t * 2
            let u = (pulse - pulse.rounded(.down) - delay) / 0.5
            guard (0..<1).contains(u) else { return Ring() }
            return Ring(scale: 0.27 + 0.73 * (1 - pow(1 - u, 3)), opacity: 0.75 * (1 - u))
        }
        let blink = min(max((t - 0.955) / 0.035, 0), 1)
        return MascotLook(lid: 1 - 0.9 * (1 - abs(2 * blink - 1)), rings: [ring(0), ring(0.1)])
    }

    nonisolated static func draw(_ look: MascotLook, in canvas: inout GraphicsContext, size: CGFloat) {
        func color(_ token: Token, _ opacity: Double = 1) -> GraphicsContext.Shading {
            let (r, g, b, _) = Token.components(token.light)
            return .color(Color(red: r, green: g, blue: b, opacity: opacity))
        }
        let k = size / 200
        canvas.scaleBy(x: k, y: k)
        canvas.translateBy(x: 0, y: height - 200)
        let light = CGPoint(x: 131, y: 19)
        var shadow = canvas
        shadow.addFilter(.blur(radius: 6))
        shadow.fill(Path(roundedRect: CGRect(x: 50, y: 166, width: 100, height: 6), cornerRadius: 3), with: .color(.black.opacity(0.25)))
        for ring in look.rings where ring.opacity > 0 {
            let r = 36 * ring.scale
            canvas.stroke(Path(ellipseIn: CGRect(x: light.x - r, y: light.y - r, width: r * 2, height: r * 2)), with: color(lightOn, ring.opacity), lineWidth: 5 * ring.scale)
        }
        if look.lit { canvas.fill(circle(light, 17), with: color(lightOn, 0.35)) }
        var antenna = Path()
        antenna.move(to: CGPoint(x: 110, y: 48))
        antenna.addCurve(to: CGPoint(x: 127, y: 23), control1: CGPoint(x: 112, y: 36), control2: CGPoint(x: 118, y: 28))
        canvas.stroke(antenna, with: color(body), style: StrokeStyle(lineWidth: 7, lineCap: .round))
        canvas.fill(circle(light, 11), with: color(body))
        canvas.fill(bodyPath, with: color(body))
        canvas.fill(sheenPath, with: .color(.white.opacity(0.08)))
        canvas.fill(circle(light, 6.5), with: color(look.lit ? lightOn : lightOff))

        var eyes = canvas
        eyes.translateBy(x: look.eyes.width, y: look.eyes.height)
        eyes.translateBy(x: 100, y: 99)
        eyes.scaleBy(x: 1, y: look.lid)
        eyes.translateBy(x: -100, y: -99)
        if look.happy {
            for (from, centre) in [(CGPoint(x: 75, y: 104), CGPoint(x: 86, y: 98)), (CGPoint(x: 111, y: 106), CGPoint(x: 122, y: 100))] {
                var arc = Path()
                arc.move(to: from)
                arc.addQuadCurve(to: CGPoint(x: from.x + 22, y: from.y), control: CGPoint(x: from.x + 11, y: from.y - 17))
                eyes.stroke(arc.applying(rotation(8, about: centre)), with: color(eyesColor), style: StrokeStyle(lineWidth: 10, lineCap: .round))
            }
        } else {
            for (x, y) in [(76.0, 76.0), (112.0, 78.0)] {
                let eye = Path(roundedRect: CGRect(x: x, y: y, width: 21, height: 44), cornerRadius: 10.5)
                eyes.fill(eye.applying(rotation(8, about: CGPoint(x: x + 10.5, y: y + 22))), with: color(eyesColor))
            }
        }
    }

    private nonisolated static func circle(_ c: CGPoint, _ r: CGFloat) -> Path {
        Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2))
    }

    private nonisolated static func rotation(_ degrees: CGFloat, about c: CGPoint) -> CGAffineTransform {
        CGAffineTransform(translationX: c.x, y: c.y).rotated(by: degrees * .pi / 180).translatedBy(x: -c.x, y: -c.y)
    }

    private nonisolated static let bodyPath: Path = {
        var p = Path()
        p.move(to: CGPoint(x: 26, y: 130))
        p.addCurve(to: CGPoint(x: 104, y: 46), control1: CGPoint(x: 26, y: 78), control2: CGPoint(x: 60, y: 46))
        p.addCurve(to: CGPoint(x: 178, y: 128), control1: CGPoint(x: 150, y: 46), control2: CGPoint(x: 178, y: 80))
        p.addCurve(to: CGPoint(x: 150, y: 162), control1: CGPoint(x: 178, y: 152), control2: CGPoint(x: 166, y: 166))
        p.addCurve(to: CGPoint(x: 120, y: 150), control1: CGPoint(x: 138, y: 159), control2: CGPoint(x: 134, y: 148))
        p.addCurve(to: CGPoint(x: 88, y: 168), control1: CGPoint(x: 106, y: 152), control2: CGPoint(x: 104, y: 168))
        p.addCurve(to: CGPoint(x: 56, y: 154), control1: CGPoint(x: 72, y: 168), control2: CGPoint(x: 70, y: 152))
        p.addCurve(to: CGPoint(x: 26, y: 130), control1: CGPoint(x: 42, y: 156), control2: CGPoint(x: 26, y: 156))
        p.closeSubpath()
        return p
    }()

    private nonisolated static let sheenPath: Path = {
        var p = Path()
        p.move(to: CGPoint(x: 40, y: 112))
        p.addCurve(to: CGPoint(x: 98, y: 56), control1: CGPoint(x: 42, y: 80), control2: CGPoint(x: 66, y: 58))
        p.addCurve(to: CGPoint(x: 54, y: 114), control1: CGPoint(x: 78, y: 66), control2: CGPoint(x: 60, y: 86))
        p.addCurve(to: CGPoint(x: 40, y: 112), control1: CGPoint(x: 52, y: 124), control2: CGPoint(x: 40, y: 124))
        p.closeSubpath()
        return p
    }()
}
