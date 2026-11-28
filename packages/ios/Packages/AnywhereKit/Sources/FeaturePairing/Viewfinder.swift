import DesignSystem
import SwiftUI

/// The scan screen's window onto the camera: the picture dimmed all round a 240 pt square with white corner brackets, and
/// what to do under it.
@MainActor
struct Viewfinder: View {
    static let size: CGFloat = 240
    let hint: String
    let failed: Bool

    var body: some View {
        GeometryReader { screen in
            let hole = CGRect(x: (screen.size.width - Self.size) / 2, y: screen.size.height / 2 - Self.size / 2 - 20, width: Self.size, height: Self.size)
            ZStack {
                Dim(hole: hole).fill(.black.opacity(0.5), style: FillStyle(eoFill: true))
                Brackets().stroke(.white, lineWidth: 4).frame(width: hole.width, height: hole.height).position(x: hole.midX, y: hole.midY)
                Text(hint)
                    .font(.system(size: 17))
                    .foregroundStyle(failed ? Palette.failedText.color : .white)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 40)
                    .position(x: screen.size.width / 2, y: hole.maxY + 36)
            }
        }
        .ignoresSafeArea()
        .accessibilityElement(children: .combine)
    }

    /// Everything but the square, to be filled with the even-odd rule.
    private struct Dim: Shape {
        let hole: CGRect

        func path(in rect: CGRect) -> Path {
            var path = Path(rect.insetBy(dx: -900, dy: -900))
            path.addRoundedRect(in: hole, cornerSize: CGSize(width: 28, height: 28), style: .continuous)
            return path
        }
    }

    /// An L of 48 pt on each corner, turning on the square's 28 pt radius.
    private struct Brackets: Shape {
        func path(in rect: CGRect) -> Path {
            let r = rect.insetBy(dx: 2, dy: 2), arm: CGFloat = 48, radius: CGFloat = 28
            var path = Path()
            for (start, corner, end) in [
                (CGPoint(x: r.minX, y: r.minY + arm), CGPoint(x: r.minX, y: r.minY), CGPoint(x: r.minX + arm, y: r.minY)),
                (CGPoint(x: r.maxX - arm, y: r.minY), CGPoint(x: r.maxX, y: r.minY), CGPoint(x: r.maxX, y: r.minY + arm)),
                (CGPoint(x: r.maxX, y: r.maxY - arm), CGPoint(x: r.maxX, y: r.maxY), CGPoint(x: r.maxX - arm, y: r.maxY)),
                (CGPoint(x: r.minX + arm, y: r.maxY), CGPoint(x: r.minX, y: r.maxY), CGPoint(x: r.minX, y: r.maxY - arm)),
            ] {
                path.move(to: start)
                path.addArc(tangent1End: corner, tangent2End: end, radius: radius)
                path.addLine(to: end)
            }
            return path
        }
    }
}

/// The scan screen's buttons, always on black.
struct ScanButtonStyle: ButtonStyle {
    enum Kind { case glass, light, plain }
    let kind: Kind

    init(_ kind: Kind) {
        self.kind = kind
    }

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle(kind == .light ? Token.fixed(0x171717ff).color : .white)
            .padding(.horizontal, kind == .light ? 24 : 22)
            .frame(height: kind == .plain ? HitTarget.minimum : 48)
            .background(kind == .glass ? Color.white.opacity(0.16) : kind == .light ? Token.fixed(0xebebebff).color : .clear, in: Capsule())
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
