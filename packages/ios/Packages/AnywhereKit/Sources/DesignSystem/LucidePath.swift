import Foundation
import SwiftUI

/// Turns an SVG body on Lucide's 24-unit grid (`path`, `circle`, `ellipse`, `rect`, `line`, `polyline`, `polygon`) into
/// one `Path` in those units. Only geometry is read: Lucide strokes everything the same way.
public enum LucidePath {
    public struct Failure: Error, Equatable {
        public let message: String
    }

    public nonisolated static func parse(_ svg: String) throws(Failure) -> Path {
        var path = Path()
        for element in elements(svg) {
            let a = element.attributes
            func number(_ name: String, _ fallback: CGFloat? = nil) throws(Failure) -> CGFloat {
                if let text = a[name], let value = Double(text) { return CGFloat(value) }
                if let fallback { return fallback }
                throw Failure(message: "<\(element.name)> has no \(name)")
            }
            switch element.name {
            case "path":
                guard let d = a["d"] else { throw Failure(message: "<path> has no d") }
                var data = PathData(d)
                try data.add(to: &path)
            case "circle":
                let r = try number("r")
                path.addEllipse(in: CGRect(x: try number("cx", 0) - r, y: try number("cy", 0) - r, width: r * 2, height: r * 2))
            case "ellipse":
                let rx = try number("rx"), ry = try number("ry")
                path.addEllipse(in: CGRect(x: try number("cx", 0) - rx, y: try number("cy", 0) - ry, width: rx * 2, height: ry * 2))
            case "rect":
                let rect = CGRect(x: try number("x", 0), y: try number("y", 0), width: try number("width"), height: try number("height"))
                let rx = a["rx"].flatMap(Double.init).map { CGFloat($0) }
                let ry = a["ry"].flatMap(Double.init).map { CGFloat($0) }
                let corner = CGSize(width: min(rx ?? ry ?? 0, rect.width / 2), height: min(ry ?? rx ?? 0, rect.height / 2))
                if corner == .zero { path.addRect(rect) } else { path.addRoundedRect(in: rect, cornerSize: corner, style: .circular) }
            case "line":
                path.move(to: CGPoint(x: try number("x1", 0), y: try number("y1", 0)))
                path.addLine(to: CGPoint(x: try number("x2", 0), y: try number("y2", 0)))
            case "polyline", "polygon":
                var scanner = Scanner(a["points"] ?? "")
                var points: [CGPoint] = []
                while !scanner.atEnd {
                    points.append(CGPoint(x: try scanner.number(), y: try scanner.number()))
                }
                guard let first = points.first else { throw Failure(message: "<\(element.name)> has no points") }
                path.move(to: first)
                for point in points.dropFirst() { path.addLine(to: point) }
                if element.name == "polygon" { path.closeSubpath() }
            default:
                throw Failure(message: "unknown element <\(element.name)>")
            }
        }
        return path
    }

    private struct Element {
        let name: String
        let attributes: [String: String]
    }

    private nonisolated static func elements(_ svg: String) -> [Element] {
        svg.split(separator: "<").compactMap { chunk in
            let body = chunk.prefix { $0 != ">" }.trimmingCharacters(in: CharacterSet(charactersIn: "/ "))
            guard let name = body.split(separator: " ").first.map(String.init), !name.isEmpty else { return nil }
            var attributes: [String: String] = [:]
            var rest = body.dropFirst(name.count)[...]
            while let eq = rest.firstIndex(of: "=") {
                let key = rest[..<eq].trimmingCharacters(in: .whitespaces)
                guard let open = rest[eq...].firstIndex(of: "\""), let close = rest[rest.index(after: open)...].firstIndex(of: "\"") else { break }
                attributes[key] = String(rest[rest.index(after: open)..<close])
                rest = rest[rest.index(after: close)...]
            }
            return Element(name: name, attributes: attributes)
        }
    }

    /// Numbers as SVG writes them: signs and dots start a new one (`.705.705`, `1-1.2`), commas and spaces separate.
    struct Scanner {
        private let chars: [UInt8]
        private var at = 0

        init(_ text: String) { chars = Array(text.utf8) }

        var atEnd: Bool {
            mutating get {
                skip()
                return at >= chars.count
            }
        }

        mutating func skip() {
            while at < chars.count, chars[at] == UInt8(ascii: " ") || chars[at] == UInt8(ascii: ",") || chars[at] == 10 || chars[at] == 9 { at += 1 }
        }

        /// The next command letter, if one is next.
        mutating func command() -> Character? {
            skip()
            guard at < chars.count, (chars[at] | 0x20) >= UInt8(ascii: "a"), (chars[at] | 0x20) <= UInt8(ascii: "z"), chars[at] | 0x20 != UInt8(ascii: "e")
            else { return nil }
            defer { at += 1 }
            return Character(UnicodeScalar(chars[at]))
        }

        mutating func number() throws(Failure) -> CGFloat {
            skip()
            let start = at
            if at < chars.count, chars[at] == UInt8(ascii: "-") || chars[at] == UInt8(ascii: "+") { at += 1 }
            var dot = false
            while at < chars.count {
                let c = chars[at]
                if c >= UInt8(ascii: "0") && c <= UInt8(ascii: "9") {
                    at += 1
                } else if c == UInt8(ascii: "."), !dot {
                    dot = true
                    at += 1
                } else if c | 0x20 == UInt8(ascii: "e") {
                    at += 1
                    if at < chars.count, chars[at] == UInt8(ascii: "-") || chars[at] == UInt8(ascii: "+") { at += 1 }
                } else {
                    break
                }
            }
            guard at > start, let value = Double(String(decoding: chars[start..<at], as: UTF8.self)) else {
                throw Failure(message: "expected a number at \(start)")
            }
            return CGFloat(value)
        }

        /// An arc flag: a single 0 or 1, which may run into the next number (`a1 1 0 011 1`).
        mutating func flag() throws(Failure) -> Bool {
            skip()
            guard at < chars.count, chars[at] == UInt8(ascii: "0") || chars[at] == UInt8(ascii: "1") else { throw Failure(message: "expected a flag at \(at)") }
            defer { at += 1 }
            return chars[at] == UInt8(ascii: "1")
        }
    }

    /// A `d` attribute's commands, absolute and relative, with implicit repeats.
    private struct PathData {
        var scanner: Scanner

        init(_ d: String) { scanner = Scanner(d) }

        mutating func add(to path: inout Path) throws(Failure) {
            var current = CGPoint.zero, start = CGPoint.zero
            /// The last curve's second control point, for `S` and `T`.
            var lastCubic: CGPoint?, lastQuad: CGPoint?
            var command: Character?
            while !scanner.atEnd {
                if let next = scanner.command() {
                    command = next
                } else if command == "M" {
                    command = "L"
                } else if command == "m" {
                    command = "l"
                }
                guard let c = command else { throw Failure(message: "path data doesn't start with a command") }
                let relative = c.isLowercase
                func point() throws(Failure) -> CGPoint {
                    let x = try scanner.number(), y = try scanner.number()
                    return relative ? CGPoint(x: current.x + x, y: current.y + y) : CGPoint(x: x, y: y)
                }
                var cubic: CGPoint?, quad: CGPoint?
                switch c.uppercased() {
                case "M":
                    current = try point()
                    start = current
                    path.move(to: current)
                case "L":
                    current = try point()
                    path.addLine(to: current)
                case "H":
                    let x = try scanner.number()
                    current.x = relative ? current.x + x : x
                    path.addLine(to: current)
                case "V":
                    let y = try scanner.number()
                    current.y = relative ? current.y + y : y
                    path.addLine(to: current)
                case "C":
                    let c1 = try point(), c2 = try point(), end = try point()
                    path.addCurve(to: end, control1: c1, control2: c2)
                    cubic = c2
                    current = end
                case "S":
                    let c1 = lastCubic.map { reflect($0, about: current) } ?? current
                    let c2 = try point(), end = try point()
                    path.addCurve(to: end, control1: c1, control2: c2)
                    cubic = c2
                    current = end
                case "Q":
                    let control = try point(), end = try point()
                    path.addQuadCurve(to: end, control: control)
                    quad = control
                    current = end
                case "T":
                    let control = lastQuad.map { reflect($0, about: current) } ?? current
                    let end = try point()
                    path.addQuadCurve(to: end, control: control)
                    quad = control
                    current = end
                case "A":
                    let rx = try scanner.number(), ry = try scanner.number(), rotation = try scanner.number()
                    let large = try scanner.flag(), sweep = try scanner.flag()
                    let end = try point()
                    Arc.add(to: &path, from: current, to: end, radii: CGSize(width: rx, height: ry), rotation: rotation, large: large, sweep: sweep)
                    current = end
                case "Z":
                    path.closeSubpath()
                    current = start
                    // A command letter must follow Z.
                    command = nil
                default:
                    throw Failure(message: "unknown path command \(c)")
                }
                lastCubic = cubic
                lastQuad = quad
            }
        }

        private func reflect(_ p: CGPoint, about c: CGPoint) -> CGPoint { CGPoint(x: 2 * c.x - p.x, y: 2 * c.y - p.y) }
    }

    /// SVG's endpoint arc (SVG 1.1 F.6.5) as cubic Béziers of at most a quarter turn each.
    enum Arc {
        static func add(to path: inout Path, from p0: CGPoint, to p1: CGPoint, radii: CGSize, rotation degrees: CGFloat, large: Bool, sweep: Bool) {
            var rx = abs(radii.width), ry = abs(radii.height)
            guard rx > 0, ry > 0, p0 != p1 else { return path.addLine(to: p1) }
            let phi = degrees * .pi / 180, cosPhi = cos(phi), sinPhi = sin(phi)
            let dx = (p0.x - p1.x) / 2, dy = (p0.y - p1.y) / 2
            let x1 = cosPhi * dx + sinPhi * dy, y1 = -sinPhi * dx + cosPhi * dy
            let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
            if lambda > 1 {
                rx *= lambda.squareRoot()
                ry *= lambda.squareRoot()
            }
            let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
            let den = rx * rx * y1 * y1 + ry * ry * x1 * x1
            let k = (large == sweep ? -1 : 1) * (max(0, num) / den).squareRoot()
            let cx1 = k * rx * y1 / ry, cy1 = -k * ry * x1 / rx
            let cx = cosPhi * cx1 - sinPhi * cy1 + (p0.x + p1.x) / 2
            let cy = sinPhi * cx1 + cosPhi * cy1 + (p0.y + p1.y) / 2
            func angle(_ ux: CGFloat, _ uy: CGFloat, _ vx: CGFloat, _ vy: CGFloat) -> CGFloat {
                atan2(ux * vy - uy * vx, ux * vx + uy * vy)
            }
            let theta = angle(1, 0, (x1 - cx1) / rx, (y1 - cy1) / ry)
            var delta = angle((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry)
            if !sweep && delta > 0 { delta -= 2 * .pi }
            if sweep && delta < 0 { delta += 2 * .pi }

            let segments = max(1, Int((abs(delta) / (.pi / 2)).rounded(.up)))
            let step = delta / CGFloat(segments)
            let t = 4 / 3 * tan(step / 4)
            func onEllipse(_ a: CGFloat) -> CGPoint {
                CGPoint(x: cx + rx * cos(a) * cosPhi - ry * sin(a) * sinPhi, y: cy + rx * cos(a) * sinPhi + ry * sin(a) * cosPhi)
            }
            func derivative(_ a: CGFloat) -> CGPoint {
                CGPoint(x: -rx * sin(a) * cosPhi - ry * cos(a) * sinPhi, y: -rx * sin(a) * sinPhi + ry * cos(a) * cosPhi)
            }
            var a = theta
            for i in 0..<segments {
                let b = a + step
                let start = onEllipse(a), d0 = derivative(a), d1 = derivative(b)
                let end = i == segments - 1 ? p1 : onEllipse(b)
                path.addCurve(
                    to: end, control1: CGPoint(x: start.x + t * d0.x, y: start.y + t * d0.y),
                    control2: CGPoint(x: end.x - t * d1.x, y: end.y - t * d1.y))
                a = b
            }
        }
    }
}

extension Lucide {
    /// The icon's outline on the 24-unit grid. Every glyph parses; the tests hold that.
    public var path: Path {
        (try? LucidePath.parse(svg)) ?? Path()
    }
}
