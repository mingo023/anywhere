import AppKit
import SwiftUI

final class Controller: ObservableObject {
    @Published private(set) var island = Island()
    @Published private(set) var notch = Notch(x: 0, top: 0, width: 0, height: 32)
    private let panel = Panel()
    private var turn = 0
    private var linger: DispatchWorkItem?

    init() {
        panel.contentView = Host(rootView: IslandView(controller: self))
        // Clicking away leaves the reply field with nowhere to type.
        _ = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: panel, queue: .main) { [weak self] _ in
            self?.change { if $0.stage == .reply { $0.collapse() } }
        }
    }

    func show(_ asks: [Ask]) {
        change { $0.show(asks) }
    }

    /// Springs the island to its new stage from wherever it is now, ordering the panel in on the first ask and out after the last.
    func change(_ step: (inout Island) -> Void) {
        var next = island
        step(&next)
        guard next != island else { return }
        if !panel.isVisible, next.current != nil, let found = Notch.find() {
            notch = found
            panel.place(found)
            panel.orderFrontRegardless()
        }
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        withAnimation(reduce ? .easeOut(duration: 0.15) : .spring(response: 0.35, dampingFraction: 1)) {
            island = next
        }
        if next.stage != .reply && panel.isKeyWindow {
            // A non-activating panel hands typing back to the frontmost app once it leaves the screen.
            panel.orderOut(nil)
            panel.orderFrontRegardless()
        }
        turn += 1
        let turn = turn
        // A critically damped spring with a 0.35s response settles within this.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { [self] in
            if self.turn == turn && island.current == nil {
                panel.orderOut(nil)
            }
        }
    }

    /// The open island waits a moment after the pointer leaves, so crossing a gap doesn't close it.
    func hover(_ inside: Bool) {
        linger?.cancel()
        if inside {
            return change { $0.open() }
        }
        let close = DispatchWorkItem { [weak self] in
            self?.change { if $0.stage == .open { $0.close() } }
        }
        linger = close
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: close)
    }

    func reply() {
        panel.makeKey()
        change { $0.reply() }
    }

    func back() {
        change { $0.close() }
    }

    func answer(_ answer: Answer) {
        var answered: Ask?
        change { answered = $0.answer(answer) }
        guard let ask = answered else { return }
        var message = ["agent": ask.agent, "request": ask.request]
        switch answer {
        case .allow: message["answer"] = "allow"
        case .deny: message["answer"] = "deny"
        case .reply(let text): (message["answer"], message["text"]) = ("reply", text)
        }
        say(message)
        // Long enough to read how it went.
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            self?.change { $0.next() }
        }
    }

    func open(_ ask: Ask) {
        say(["answer": "open", "agent": ask.agent])
        change { $0.collapse() }
    }

    private func say(_ message: [String: String]) {
        guard var line = try? JSONSerialization.data(withJSONObject: message) else { return }
        line.append(0x0A)
        FileHandle.standardOutput.write(line)
    }
}
