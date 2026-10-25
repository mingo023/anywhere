#if DEBUG || BENCH
import BenchScenes
import SwiftUI
import UIKit

/// `-bench <scenario>`: one of `anywhere-bench`'s screens over fixture stores, timed frame by frame on iOS. It streams
/// the scenario's updates one per display frame, then flicks the main list, and prints a `IOSBENCH|…` line and exits.
@MainActor
enum BenchRun {
    static var scenario: BenchScenes.Scenario? {
        let args = ProcessInfo.processInfo.arguments
        guard let at = args.firstIndex(of: "-bench"), at + 1 < args.count else { return nil }
        return (BenchScenes.scenarios + BenchScenes.extras).first { $0.name == args[at + 1] }
    }

    /// Made once: the App's body runs more than once, and each scenario builds thousands of items.
    static let driver = scenario.map { BenchDriver($0) }
}

struct BenchView: View {
    let run: BenchDriver

    var body: some View {
        run.screen.view.onAppear { run.start() }
    }
}

/// Times each display frame by the main thread's CPU time since the last, which is what a frame has to fit in.
@MainActor
final class BenchDriver {
    let scenario: BenchScenes.Scenario
    let screen: BenchScenes.Screen
    private let made: UInt64
    private var load = 0.0
    private var link: CADisplayLink?
    private var last: UInt64 = 0
    private var lastStamp: CFTimeInterval = 0
    private var phase = Phase.streaming(0)
    private var stream = Frames()
    private var scroll = Frames()
    private var moved: CGFloat = 0

    private enum Phase {
        case streaming(Int)
        case scrolling(Int)
    }

    private struct Frames {
        var cpu: [Double] = []
        var hitches = 0

        func line() -> String {
            guard !cpu.isEmpty else { return "-" }
            let s = cpu.sorted()
            func q(_ p: Double) -> String { String(format: "%.1f", s[min(Int(Double(s.count) * p), s.count - 1)]) }
            return "\(q(0.5))/\(q(0.95))/\(String(format: "%.1f", s.last!)) hitches \(hitches)/\(cpu.count)"
        }
    }

    init(_ scenario: BenchScenes.Scenario) {
        self.scenario = scenario
        made = Self.clock()
        screen = scenario.make()
    }

    func start() {
        guard link == nil else { return }
        // Let off-main work land (highlighting, a long markdown parse) before timing frames.
        Timer.scheduledTimer(withTimeInterval: 1.5, repeats: false) { _ in
            MainActor.assumeIsolated { self.begin() }
        }
    }

    private func begin() {
        load = Double(Self.clock() - made) / 1e6
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
        NSLog("IOSBENCH-progress begin after %.0f ms", load)
    }

    @objc private func tick(_ link: CADisplayLink) {
        let now = Self.clock()
        if last != 0 {
            let cpu = Double(now - last) / 1e6
            let late = link.timestamp - lastStamp > (link.targetTimestamp - link.timestamp) * 1.5
            switch phase {
            case .streaming:
                stream.cpu.append(cpu)
                if late { stream.hitches += 1 }
            case .scrolling:
                scroll.cpu.append(cpu)
                if late { scroll.hitches += 1 }
            }
        }
        last = now
        lastStamp = link.timestamp
        if (stream.cpu.count + scroll.cpu.count) % 120 == 0 { NSLog("IOSBENCH-progress %d stream, %d scroll frames", stream.cpu.count, scroll.cpu.count) }
        switch phase {
        case .streaming(let n) where n < screen.updates.count:
            screen.updates[n]()
            phase = .streaming(n + 1)
        case .streaming:
            phase = .scrolling(0)
        case .scrolling(let n) where n < Self.scrollFrames:
            flick(by: scenario.up ? -30 : 30)
            phase = .scrolling(n + 1)
        case .scrolling:
            finish()
        }
    }

    private lazy var list = mainScroll()

    private func flick(by dy: CGFloat) {
        guard let list else { return }
        let top = -list.adjustedContentInset.top
        let bottom = max(list.contentSize.height - list.bounds.height + list.adjustedContentInset.bottom, top)
        let y = min(max(list.contentOffset.y + dy, top), bottom)
        moved += abs(y - list.contentOffset.y)
        list.contentOffset.y = y
    }

    private func mainScroll() -> UIScrollView? {
        let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
        func all(_ v: UIView) -> [UIScrollView] { ((v as? UIScrollView).map { [$0] } ?? []) + v.subviews.flatMap(all) }
        return window.flatMap { all($0).max { $0.contentSize.height < $1.contentSize.height } }
    }

    private func finish() {
        link?.invalidate()
        let fromEnd = mainScroll().map { $0.contentSize.height - $0.bounds.maxY + $0.adjustedContentInset.bottom } ?? 0
        let line = [
            "IOSBENCH", scenario.name, String(format: "load %.0f ms", load), "stream cpu p50/p95/max \(stream.line())",
            "scroll cpu p50/p95/max \(scroll.line()) over \(Int(moved)) pt, ending \(Int(fromEnd)) pt above the end", String(format: "memory %.0f MB", Self.footprint()),
            "\(UIScreen.main.maximumFramesPerSecond) Hz",
        ].joined(separator: "|")
        print(line)
        NSLog("%@", line)
        exit(0)
    }

    static let scrollFrames = Int(ProcessInfo.processInfo.environment["UIBENCH_SCROLL"] ?? "") ?? 240

    /// The main thread's own CPU time, so the host's other processes don't blur the numbers.
    static func clock() -> UInt64 { clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) }

    static func footprint() -> Double {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
        }
        return result == KERN_SUCCESS ? Double(info.phys_footprint) / 1_048_576 : 0
    }
}
#endif

#if DEBUG || BENCH
import ActivityKit
import ActivityModel
import Stores

/// `-check live-activity`: starts a Live Activity as a Mac turning busy would, and logs how long the main thread
/// waited and the longest gap between its frames while ActivityKit worked, then whether the activity is running.
/// The request asks for a push token, so the app needs its entitlements: sign it, ad hoc (`CODE_SIGN_IDENTITY=-`) on a simulator.
@MainActor
enum LiveActivityCheck {
    static var requested: Bool { ProcessInfo.processInfo.arguments.contains("-check") }

    static func run() {
        Task {
            // ActivityKit starts one only for an app in front.
            try? await Task.sleep(for: .seconds(1))
            check()
        }
    }

    private static func check() {
        let ticker = Ticker()
        let start = CACurrentMediaTime()
        PushCenter.shared.startActivity?(MacActivity(mac: "check", macName: "Check Mac"), .init(working: 1, needsYou: 0, unseen: 0))
        let call = (CACurrentMediaTime() - start) * 1000
        Task {
            try? await Task.sleep(for: .seconds(3))
            let active = Activity<MacActivity>.activities.filter { $0.attributes.mac == "check" && $0.activityState == .active }.count
            NSLog("IOSCHECK|live-activity|start returned in %.1f ms|longest frame gap %.0f ms|%d active", call, ticker.longest * 1000, active)
            for activity in Activity<MacActivity>.activities { await activity.end(nil, dismissalPolicy: .immediate) }
            exit(0)
        }
    }

    private final class Ticker: NSObject {
        var longest: CFTimeInterval = 0
        private var last: CFTimeInterval = 0
        private var link: CADisplayLink?

        override init() {
            super.init()
            link = CADisplayLink(target: self, selector: #selector(tick))
            link?.add(to: .main, forMode: .common)
        }

        @objc private func tick(_ link: CADisplayLink) {
            if last != 0 { longest = max(longest, link.timestamp - last) }
            last = link.timestamp
        }
    }
}
#endif
