import CoreGraphics
import SwiftUI
import Testing
@testable import DesignSystem

@Suite("Lucide paths")
struct LucidePathTests {
    private func box(_ svg: String) throws -> CGRect {
        try LucidePath.parse(svg).boundingRect
    }

    private func close(_ a: CGRect, _ b: CGRect, within: CGFloat = 0.01) -> Bool {
        abs(a.minX - b.minX) < within && abs(a.minY - b.minY) < within && abs(a.maxX - b.maxX) < within && abs(a.maxY - b.maxY) < within
    }

    @Test("every icon parses and stays on its 24-unit grid")
    func everyIcon() throws {
        for icon in Lucide.allCases {
            let box = try LucidePath.parse(icon.svg).boundingRect
            #expect(!box.isEmpty || box.width > 0 || box.height > 0, "\(icon) drew nothing")
            #expect(box.minX > -0.5 && box.minY > -0.5 && box.maxX < 24.5 && box.maxY < 24.5, "\(icon) \(box)")
        }
    }

    @Test("numbers run together: a sign or a second dot starts the next")
    func runTogether() throws {
        #expect(close(try box("<path d=\"M1-1.5.5.5\"/>"), CGRect(x: 0.5, y: -1.5, width: 0.5, height: 2)))
        #expect(close(try box("<path d=\"M2 2l.5.5-1-1\"/>"), CGRect(x: 1.5, y: 1.5, width: 1, height: 1)))
        #expect(close(try box("<path d=\"M0 0L1e1 2E-1\"/>"), CGRect(x: 0, y: 0, width: 10, height: 0.2)))
    }

    @Test("coordinates after a move are lines, relative after a relative move")
    func implicitLines() throws {
        #expect(close(try box("<path d=\"m2 2 4 0 0 4\"/>"), CGRect(x: 2, y: 2, width: 4, height: 4)))
        #expect(close(try box("<path d=\"M2 2 6 2 6 6\"/>"), CGRect(x: 2, y: 2, width: 4, height: 4)))
    }

    @Test("horizontal and vertical lines and a dot at a point")
    func hvAndDots() throws {
        #expect(close(try box("<path d=\"M3 4h5v6H3V4\"/>"), CGRect(x: 3, y: 4, width: 5, height: 6)))
        #expect(close(try box("<path d=\"M12 17h.01\"/>"), CGRect(x: 12, y: 17, width: 0.01, height: 0)))
    }

    @Test("an arc's flags may run into the next number, and it bulges to the side its sweep says")
    func arcs() throws {
        // A half circle of radius 5 from (0,10) to (10,10): sweep 1 goes up through (5,5), sweep 0 down through (5,15).
        #expect(close(try box("<path d=\"M0 10a5 5 0 0 1 10 0\"/>"), CGRect(x: 0, y: 5, width: 10, height: 5), within: 0.05))
        #expect(close(try box("<path d=\"M0 10a5 5 0 0 0 10 0\"/>"), CGRect(x: 0, y: 10, width: 10, height: 5), within: 0.05))
        #expect(close(try box("<path d=\"M0 10a5 5 0 0110 0\"/>"), CGRect(x: 0, y: 5, width: 10, height: 5), within: 0.05))
        // Radii too small to reach are scaled up to just reach.
        #expect(close(try box("<path d=\"M0 10A1 1 0 0 1 10 10\"/>"), CGRect(x: 0, y: 5, width: 10, height: 5), within: 0.05))
    }

    @Test("a large arc takes the long way round")
    func largeArc() throws {
        // Three quarters of a circle of radius 4 centred on (12,12), from (16,12) to (12,8).
        let long = try box("<path d=\"M16 12a4 4 0 1 1-4-4\"/>")
        #expect(close(long, CGRect(x: 8, y: 8, width: 8, height: 8), within: 0.05))
    }

    @Test("smooth curves reflect the last control point")
    func smoothCurves() throws {
        let cubic = try LucidePath.parse("<path d=\"M0 0C0 4 4 4 4 0S8-4 8 0\"/>").boundingRect
        #expect(cubic.minY < -2 && cubic.maxY > 2)
        let quad = try LucidePath.parse("<path d=\"M0 0Q2 4 4 0T8 0\"/>").boundingRect
        #expect(quad.minY < -1 && quad.maxY > 1)
    }

    @Test("shapes: circle, ellipse, rounded rect, line, polyline, polygon")
    func shapes() throws {
        #expect(close(try box("<circle cx=\"12\" cy=\"12\" r=\"10\"/>"), CGRect(x: 2, y: 2, width: 20, height: 20)))
        #expect(close(try box("<ellipse cx=\"12\" cy=\"5\" rx=\"9\" ry=\"3\"/>"), CGRect(x: 3, y: 2, width: 18, height: 6)))
        #expect(close(try box("<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/>"), CGRect(x: 3, y: 3, width: 18, height: 18)))
        #expect(close(try box("<line x1=\"6\" x2=\"6\" y1=\"3\" y2=\"15\"/>"), CGRect(x: 6, y: 3, width: 0, height: 12)))
        #expect(close(try box("<polyline points=\"22 12 16 12 14 15\"/>"), CGRect(x: 14, y: 12, width: 8, height: 3)))
        #expect(close(try box("<polygon points=\"2,2 10,2 6,8\"/>"), CGRect(x: 2, y: 2, width: 8, height: 6)))
    }

    @Test("a body it can't read is an error, not a blank icon")
    func rejects() {
        #expect(throws: LucidePath.Failure.self) { try LucidePath.parse("<path d=\"M0 0 L\"/>") }
        #expect(throws: LucidePath.Failure.self) { try LucidePath.parse("<path d=\"12 12\"/>") }
        #expect(throws: LucidePath.Failure.self) { try LucidePath.parse("<path d=\"M0 0a1 1 0 2 1 2 2\"/>") }
        #expect(throws: LucidePath.Failure.self) { try LucidePath.parse("<text x=\"1\"/>") }
        #expect(throws: LucidePath.Failure.self) { try LucidePath.parse("<circle cx=\"1\"/>") }
    }
}
