import AppKit
import BenchScenes
import DesignSystem
import Foundation
import SwiftUI

/// `anywhere-bench ui [scenario] [--png <dir>]`: the real screens over fixture stores in an offscreen `NSHostingView`.
/// With no scenario it runs each in its own process, so memory is the scenario's alone, and prints one table.
/// Body counts need a build with `-Xswiftc -DPERF_COUNTERS`.
@MainActor
enum UIBench {
    typealias Scenario = BenchScenes.Scenario
    typealias Screen = BenchScenes.Screen

    static func main(_ args: [String]) -> Int32 {
        var args = args
        var png: URL?
        if let at = args.firstIndex(of: "--png"), at + 1 < args.count {
            png = URL(fileURLWithPath: args[at + 1], isDirectory: true)
            args.removeSubrange(at...(at + 1))
        }
        guard let name = args.first else { return all(png) }
        guard let scenario = (BenchScenes.scenarios + BenchScenes.extras).first(where: { $0.name == name }) else {
            print("unknown scenario \(name); one of \(BenchScenes.scenarios.map(\.name).joined(separator: ", "))")
            return 2
        }
        NSApplication.shared.setActivationPolicy(.prohibited)
        let result = run(scenario, png: png)
        print(result.line)
        guard let gaps = result.endGaps else { return 0 }
        // A chat opens at its end and keeps to it as output streams in.
        let ok = gaps.load <= 2 && gaps.streamed <= 2
        print("FOLLOW|\(scenario.name)|opened \(Int(gaps.load)) pt from the end|after streaming \(Int(gaps.streamed)) pt from the end|\(ok ? "ok" : "fail")")
        return ok ? 0 : 1
    }

    // MARK: Running

    struct Result {
        let name: String
        let load: Double
        let frames: [Double]
        let scrolls: [Double]
        let memory: Double
        let loadBodies: [String: Double]
        let frameBodies: [String: Double]
        let scrollBodies: [String: Double]
        /// How far a bottom-anchored list sat from its end once opened, and after the updates.
        var endGaps: (load: CGFloat, streamed: CGFloat)?

        var line: String {
            func ms(_ v: [Double], _ q: Double) -> String {
                guard !v.isEmpty else { return "-" }
                let s = v.sorted()
                return String(format: "%.1f", s[min(Int(Double(s.count) * q), s.count - 1)])
            }
            func bodies(_ b: [String: Double]) -> String {
                guard BodyCount.enabled else { return "-" }
                let shown = b.filter { $0.value >= 0.05 }.sorted { $0.value > $1.value }
                return shown.isEmpty ? "none" : shown.map { "\($0.key) \(String(format: "%.1f", $0.value))" }.joined(separator: ", ")
            }
            return [
                "ROW", name, String(format: "%.1f", load), ms(frames, 0.5), ms(frames, 0.95), ms(scrolls, 0.5), ms(scrolls, 0.95),
                String(format: "%.0f", memory), bodies(loadBodies), bodies(frameBodies), bodies(scrollBodies),
            ].joined(separator: "|")
        }
    }

    static func run(_ scenario: Scenario, png: URL?) -> Result {
        let screen = scenario.make()
        let rect = NSRect(origin: .zero, size: scenario.size)
        let start = clock()
        let view = NSHostingView(rootView: screen.view.environment(\.controlActiveState, .key).frame(width: rect.width, height: rect.height))
        view.frame = rect
        let window = NSWindow(contentRect: rect, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        // Offscreen, but ordered in so SwiftUI treats the view as visible and builds lazy rows.
        window.setFrameOrigin(NSPoint(x: -10_000 - rect.width, y: -10_000))
        window.orderFrontRegardless()
        settle(view, passes: 4)
        let load = elapsed(since: start)
        let memory = footprint()
        let loadBodies = BodyCount.take().mapValues(Double.init)

        // Let off-main work land (highlighting, a long markdown parse) before what follows.
        idle(view, seconds: 1)
        if let png { snapshot(view, to: png.appendingPathComponent("\(scenario.name)-loaded.png")) }
        let loadGap = endGap(view)

        _ = BodyCount.take()
        var frames: [Double] = []
        for update in screen.updates {
            let t = clock()
            update()
            settle(view, passes: 3)
            frames.append(elapsed(since: t))
        }
        let frameBodies = BodyCount.take().mapValues { Double($0) / Double(max(screen.updates.count, 1)) }

        idle(view, seconds: 0.3)
        let streamedGap = endGap(view)
        if let png { snapshot(view, to: png.appendingPathComponent("\(scenario.name)-updated.png")) }
        _ = BodyCount.take()
        var scrolls: [Double] = []
        let steps = Int(ProcessInfo.processInfo.environment["UIBENCH_PAGES"] ?? "") ?? 12
        if let scroll = mainScroll(view) {
            // The chat follows its tail until the user drags it; a drag on the list takes it, as a finger would.
            if scenario.up { drag(window, at: NSPoint(x: rect.midX, y: rect.midY)) }
            settle(view, passes: 3)
            _ = BodyCount.take()
            for _ in 0..<steps {
                let t = clock()
                page(scroll, up: scenario.up)
                settle(view, passes: 3)
                scrolls.append(elapsed(since: t))
            }
        }
        let scrollBodies = BodyCount.take().mapValues { Double($0) / Double(max(scrolls.count, 1)) }
        if let png { snapshot(view, to: png.appendingPathComponent("\(scenario.name)-scrolled.png")) }
        window.orderOut(nil)
        return Result(
            name: scenario.name, load: load, frames: frames, scrolls: scrolls, memory: memory, loadBodies: loadBodies, frameBodies: frameBodies,
            scrollBodies: scrollBodies, endGaps: scenario.follows ? (loadGap, streamedGap) : nil)
    }

    /// One turn of the run loop (main-actor tasks, preference and state changes), then layout and a commit.
    static func settle(_ view: NSView, passes: Int) {
        for _ in 0..<passes {
            CFRunLoopRunInMode(.defaultMode, 0, false)
            view.layoutSubtreeIfNeeded()
            view.displayIfNeeded()
            CATransaction.flush()
        }
    }

    static func idle(_ view: NSView, seconds: Double) {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
            view.layoutSubtreeIfNeeded()
        }
    }

    /// The scroll view holding the most content: the list or the transcript, not a pill's.
    static func mainScroll(_ view: NSView) -> NSScrollView? {
        func all(_ v: NSView) -> [NSScrollView] { ((v as? NSScrollView).map { [$0] } ?? []) + v.subviews.flatMap(all) }
        return all(view).max { ($0.documentView?.frame.height ?? 0) < ($1.documentView?.frame.height ?? 0) }
    }

    /// How far the main scroll view's visible bottom sits above its content's end.
    static func endGap(_ view: NSView) -> CGFloat {
        guard let scroll = mainScroll(view), let doc = scroll.documentView else { return .infinity }
        let clip = scroll.contentView.bounds
        return doc.isFlipped ? doc.frame.height - clip.maxY : clip.minY
    }

    static func drag(_ window: NSWindow, at point: NSPoint) {
        for (type, dy) in [(NSEvent.EventType.leftMouseDown, 0.0), (.leftMouseDragged, 12), (.leftMouseDragged, 24), (.leftMouseUp, 24)] {
            guard let event = NSEvent.mouseEvent(
                with: type, location: NSPoint(x: point.x, y: point.y - dy), modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1)
            else { continue }
            window.sendEvent(event)
            CFRunLoopRunInMode(.defaultMode, 0, false)
        }
    }

    /// Scrolls by one viewport, as a flick would land.
    static func page(_ scroll: NSScrollView, up: Bool) {
        let clip = scroll.contentView
        var origin = clip.bounds.origin
        let flipped = scroll.documentView?.isFlipped ?? true
        origin.y += (up == flipped ? -1 : 1) * clip.bounds.height
        let maxY = max((scroll.documentView?.frame.height ?? 0) - clip.bounds.height, 0)
        origin.y = min(max(origin.y, 0), maxY)
        clip.scroll(to: origin)
        scroll.reflectScrolledClipView(clip)
    }

    /// The main thread's own CPU time, so other processes on the machine don't blur the numbers.
    static func clock() -> UInt64 { clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) }

    static func elapsed(since start: UInt64) -> Double { Double(clock() - start) / 1e6 }

    /// What the process holds in memory, as Xcode's gauge shows it, in MB.
    static func footprint() -> Double {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(task_self_trap(), task_flavor_t(TASK_VM_INFO), $0, &count) }
        }
        return result == KERN_SUCCESS ? Double(info.phys_footprint) / 1_048_576 : 0
    }

    static func snapshot(_ view: NSView, to url: URL) {
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
        view.cacheDisplay(in: view.bounds, to: rep)
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? rep.representation(using: .png, properties: [:])?.write(to: url)
    }

    /// Each scenario three times, each in a fresh process, then the table of medians.
    static func all(_ png: URL?) -> Int32 {
        let runs = 3
        var rows: [[String]] = []
        var failed = false
        for scenario in BenchScenes.scenarios {
            var takes: [[String]] = []
            for run in 0..<runs {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: CommandLine.arguments[0])
                process.arguments = ["ui", scenario.name] + (png.map { ["--png", $0.path] } ?? [])
                let out = Pipe()
                process.standardOutput = out
                process.standardError = FileHandle.nullDevice
                do { try process.run() } catch {
                    print("couldn't start \(scenario.name): \(error)")
                    return 1
                }
                let data = out.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                for line in String(decoding: data, as: UTF8.self).split(separator: "\n") {
                    if line.hasPrefix("ROW|") { takes.append(line.split(separator: "|", omittingEmptySubsequences: false).dropFirst().map(String.init)) }
                    else if line.hasPrefix("FOLLOW|") && line.hasSuffix("|fail") { failed = true; print(line) }
                    else if run == 0 { print(line) }
                }
            }
            guard let first = takes.first else { continue }
            // Numbers are each column's median; body counts are the first run's.
            rows.append(first.indices.map { c in
                let numbers = takes.compactMap { Double($0[c]) }.sorted()
                return numbers.count == takes.count && c > 0 ? String(format: "%.1f", numbers[numbers.count / 2]) : first[c]
            })
        }
        print("| Scenario | Load ms | Update p50 / p95 ms | Scroll p50 / p95 ms | Memory MB |")
        print("|---|---:|---:|---:|---:|")
        for r in rows { print("| \(r[0]) | \(r[1]) | \(r[2]) / \(r[3]) | \(r[4]) / \(r[5]) | \(r[6]) |") }
        print("\n| Scenario | Bodies to load | Bodies per update | Bodies per scroll |")
        print("|---|---|---|---|")
        for r in rows { print("| \(r[0]) | \(r[7]) | \(r[8]) | \(r[9]) |") }
        return failed ? 1 : 0
    }
}
