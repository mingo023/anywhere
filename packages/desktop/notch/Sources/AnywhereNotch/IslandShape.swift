import SwiftUI

/// Bottom corners rounded; the top flares out by `ear` into the screen's edge, as the notch does.
struct IslandShape: Shape {
    var radius: CGFloat
    var ear: CGFloat

    var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get { AnimatablePair(radius, ear) }
        set { (radius, ear) = (newValue.first, newValue.second) }
    }

    func path(in r: CGRect) -> Path {
        let body = r.insetBy(dx: ear, dy: 0)
        let radius = min(radius, body.height / 2, body.width / 2)
        var p = Path()
        p.move(to: CGPoint(x: r.minX, y: r.minY))
        p.addArc(tangent1End: CGPoint(x: body.minX, y: r.minY), tangent2End: CGPoint(x: body.minX, y: r.maxY), radius: ear)
        p.addArc(tangent1End: CGPoint(x: body.minX, y: r.maxY), tangent2End: CGPoint(x: body.midX, y: r.maxY), radius: radius)
        p.addArc(tangent1End: CGPoint(x: body.maxX, y: r.maxY), tangent2End: CGPoint(x: body.maxX, y: r.minY), radius: radius)
        p.addArc(tangent1End: CGPoint(x: body.maxX, y: r.minY), tangent2End: CGPoint(x: r.maxX, y: r.minY), radius: ear)
        p.closeSubpath()
        return p
    }
}
