import AppKit
import Client
import DesignSystem
import FeatureChat
import FeatureFiles
import FeatureGit
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

/// `anywhere-bench ui [scenario] [--png <dir>]`: the real screens over fixture stores in an offscreen `NSHostingView`.
/// With no scenario it runs each in its own process, so memory is the scenario's alone, and prints one table.
/// Body counts need a build with `-Xswiftc -DPERF_COUNTERS`.
@MainActor
enum UIBench {
    struct Scenario {
        let name: String
        var size = CGSize(width: 393, height: 852)
        /// Bottom-anchored: scrolling goes up.
        var up = false
        let make: @MainActor () -> Screen
    }

    struct Screen {
        let view: AnyView
        /// One live update, made ahead so building it isn't timed.
        var updates: [@MainActor () -> Void] = []
    }

    static let scenarios: [Scenario] = [
        Scenario(name: "chat-5000", up: true, make: chat),
        Scenario(name: "changes-1500", make: changes),
        Scenario(name: "diff-20k", make: { diff(sideBySide: false) }),
        Scenario(name: "diff-20k-split", size: CGSize(width: 1024, height: 768), make: { diff(sideBySide: true) }),
        Scenario(name: "file-10k", make: file),
        Scenario(name: "markdown-258k", make: markdown),
    ]

    static func main(_ args: [String]) -> Int32 {
        var args = args
        var png: URL?
        if let at = args.firstIndex(of: "--png"), at + 1 < args.count {
            png = URL(fileURLWithPath: args[at + 1], isDirectory: true)
            args.removeSubrange(at...(at + 1))
        }
        guard let name = args.first else { return all(png) }
        guard let scenario = scenarios.first(where: { $0.name == name }) else {
            print("unknown scenario \(name); one of \(scenarios.map(\.name).joined(separator: ", "))")
            return 2
        }
        NSApplication.shared.setActivationPolicy(.prohibited)
        print(run(scenario, png: png).line)
        return 0
    }

    // MARK: Scenarios

    static let agentId = "a-perf"

    static func chat() -> Screen {
        var items = Sample.chatPerf(items: Int(ProcessInfo.processInfo.environment["UIBENCH_ITEMS"] ?? "") ?? 5000)
        let seq = items.count
        items.append(TimelineItem(id: "s-u", seq: seq + 1, ts: Sample.now, body: .user(text: "Stream a long answer")))
        items.append(TimelineItem(id: "s-a", seq: seq + 2, ts: Sample.now, body: .assistant(text: "Streaming ")))
        let agent = AgentSummary(
            id: agentId, terminalId: "t-perf", title: "Perf", cwd: "/Users/mingo/dev/anywhere", provider: "claude", model: "claude-opus-5-5",
            effort: "high", access: "edits", status: .working, attached: true, epoch: 1, maxSeq: seq + 2, createdAt: Sample.now - 600_000,
            updatedAt: Sample.now, branch: "perf", tokensUsed: 48_000, contextWindow: 1_000_000)
        let host = HostModel.offline(hello: Sample.hello, messages: [.agentList(.init(agents: [agent]))])
        let store = host.chat(agentId)
        host.apply([.message(.agentTimeline(.init(agentId: agentId, epoch: 1, items: items, hasOlder: false, maxSeq: seq + 2)))])
        store.flush()
        var text = "Streaming "
        let frames = Int(ProcessInfo.processInfo.environment["UIBENCH_FRAMES"] ?? "") ?? 60
        let updates = (0..<frames).map { frame -> @MainActor () -> Void in
            text += frame % 20 == 19 ? "end.\n\n" : "word "
            let item = TimelineItem(id: "s-a", seq: seq + 2, ts: Sample.now, body: .assistant(text: text))
            return { host.apply([.message(.agentStream(.init(agentId: agentId, epoch: 1, item: item)))]) }
        }
        return Screen(view: AnyView(NavigationStack { ChatView(host: host, agentId: agentId) }), updates: updates)
    }

    static func offlineHost() -> HostModel {
        HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "bench", macName: "Mac", clientId: "bench"))
    }

    static func changes() -> Screen {
        let host = offlineHost()
        let status = Sample.gitStatus(files: 1500)
        host.git.take(status)
        // A refresh after the Mac said the tree moved: a new rev, one file's counts changed.
        let updates = (0..<20).map { n -> @MainActor () -> Void in
            var next = status
            next.rev = "moved-\(n)"
            next.files[n * 37 % next.files.count].added = (next.files[n * 37 % next.files.count].added ?? 0) + n + 1
            return { host.git.take(next) }
        }
        return Screen(view: AnyView(NavigationStack { GitView(host: host, route: .changes(mac: Sample.mac, tree: Sample.tree)) }), updates: updates)
    }

    static func diff(sideBySide: Bool) -> Screen {
        let host = offlineHost()
        let diff = Sample.diff(lines: 20_000)
        host.git.take(diff, at: .working)
        UserDefaults.standard.set(sideBySide, forKey: "git.diff.sideBySide")
        return Screen(view: AnyView(NavigationStack {
            GitView(host: host, route: .diff(mac: Sample.mac, tree: Sample.tree, path: diff.path, staged: false))
        }))
    }

    static func preview(_ path: String, text: String, language: String) -> Screen {
        let host = offlineHost()
        _ = host.files.preview(tree: Sample.fileTree, path: path)
        _ = host.files.apply(.fsRead(FsRead(
            tree: Sample.fileTree, path: path, kind: .text, size: text.utf8.count, mtime: 1, text: text, language: language)))
        return Screen(view: AnyView(NavigationStack { FilesView(host: host, route: .file(mac: Sample.mac, tree: Sample.fileTree, path: path)) }))
    }

    static func file() -> Screen {
        let lines = Sample.rustSource(bytes: 1 << 20).split(separator: "\n", omittingEmptySubsequences: false).prefix(10_000)
        UserDefaults.standard.set(false, forKey: "files.markdownSource")
        return preview("src/lanes.rs", text: lines.joined(separator: "\n"), language: "rust")
    }

    static func markdown() -> Screen {
        UserDefaults.standard.set(false, forKey: "files.markdownSource")
        return preview("docs/long.md", text: Sample.markdownDocument(bytes: 258 * 1024), language: "markdown")
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
        _ = BodyCount.take()
        var scrolls: [Double] = []
        let steps = 12
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
            name: scenario.name, load: load, frames: frames, scrolls: scrolls, memory: memory, loadBodies: loadBodies, frameBodies: frameBodies, scrollBodies: scrollBodies)
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
        for scenario in scenarios {
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
        return 0
    }
}
