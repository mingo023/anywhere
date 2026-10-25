import ActivityModel
import Client
import Foundation
import Observation
import PushSupport
import Wire

/// This phone's push registration on one Mac, and what the Live Activity and the badge need from its sessions.
@MainActor
@Observable
public final class PushStore: HostStore {
    public enum State: Hashable, Sendable {
        /// The Mac's pocketd doesn't push.
        case unsupported
        /// Waiting on notification permission or this phone's APNs token.
        case waiting
        case registering
        case registered
        case failed(String)
    }

    public private(set) var state = State.waiting

    @ObservationIgnored let connection: DaemonConnection?
    @ObservationIgnored let center: PushCenter
    @ObservationIgnored private var hello: HelloOk?
    @ObservationIgnored private var agents: [String: AgentSummary] = [:]
    @ObservationIgnored private var titles: [String: ActivityTitles.Entry] = [:]
    @ObservationIgnored private var badge: Int?
    @ObservationIgnored private var wasBusy = false
    /// Bumped by each registration, so an older one finishing late doesn't overwrite a newer one's state.
    @ObservationIgnored private var attempt = 0

    public init(connection: DaemonConnection? = nil, center: PushCenter = .shared) {
        self.connection = connection
        self.center = center
        center.attach(self)
    }

    /// The Mac's serverId, once it has said hello.
    var mac: String? { hello?.serverId }

    public func connected(_ hello: HelloOk) {
        self.hello = hello
        guard hello.has(Caps.push) else {
            state = .unsupported
            return
        }
        Task { await register() }
    }

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .agentList(let list):
            agents = Dictionary(list.agents.map { ($0.id, $0) }, uniquingKeysWith: { $1 })
            sessionsChanged()
            if let mac {
                let wanting = Self.sessions(Array(agents.values)).filter { $0.status != MacActivity.Top.working }.map(\.id)
                center.clearDelivered(mac: mac, keep: Set(wanting))
            }
            return false
        case .agentUpdate(let update):
            agents[update.agent.id] = update.agent.status == .closed ? nil : update.agent
        default:
            return false
        }
        sessionsChanged()
        return false
    }

    /// Sends `push.register` once this phone may be pushed to, then the Mac's Live Activity token if one waits.
    func register() async {
        guard let connection, let hello, hello.has(Caps.push) else { return }
        attempt += 1
        let mine = attempt
        do {
            guard let message = try await center.registration(mac: hello.serverId) else {
                if mine == attempt { state = .waiting }
                return
            }
            if mine == attempt { state = .registering }
            _ = try await connection.request(.pushRegister(message))
            guard mine == attempt else { return }
            state = .registered
            sendActivity()
        } catch {
            if mine == attempt { state = .failed(Self.describe(error)) }
        }
    }

    func sendPrefs() {
        guard state == .registered, let connection else { return }
        let prefs = center.prefs
        Task { _ = try? await connection.request(.pushPrefs(.init(prefs: prefs))) }
    }

    func sendActivity() {
        guard state == .registered, let connection, let mac, let activity = center.activities[mac] else { return }
        let center = center
        Task {
            guard let handle = try? await center.handle(token: activity.token, kind: .activity) else { return }
            _ = try? await connection.request(.pushActivity(.init(activityId: activity.id, handle: handle)))
        }
    }

    func sendEnded(_ id: String) {
        guard state == .registered, let connection else { return }
        Task { _ = try? await connection.request(.pushActivity(.init(activityId: id, ended: true))) }
    }

    /// Asks the Mac to send this phone a test alert now.
    public func sendTest() async throws {
        guard let connection else { throw DaemonError.offline }
        _ = try await connection.request(.pushTest)
    }

    /// Stops this Mac pushing to this phone, and forgets its sealing key.
    public func unregister() async throws {
        guard let connection, let mac else { throw DaemonError.offline }
        _ = try await connection.request(.pushUnregister)
        center.forget(mac: mac)
        state = .waiting
    }

    /// Keeps the widget's titles, the badge and the Live Activity in step with the Mac's sessions.
    private func sessionsChanged() {
        guard let hello else { return }
        let summaries = Array(agents.values)
        let titles = Self.titles(summaries)
        if titles != self.titles {
            self.titles = titles
            ActivityTitles(defaults: center.defaults).save(mac: hello.serverId, titles)
        }
        let sessions = Self.sessions(summaries)
        let badge = sessions.count { $0.status != MacActivity.Top.working }
        if badge != self.badge {
            self.badge = badge
            center.badge(mac: hello.serverId, count: badge)
        }
        let state = MacActivity.ContentState.build(sessions, asks: [], details: false)
        if Self.turnedBusy(was: wasBusy, now: state), center.liveActivities, hello.has(Caps.push) {
            center.startActivity?(MacActivity(mac: hello.serverId, macName: hello.hostname), state)
        }
        wasBusy = state.isBusy
    }

    /// The app starts a Mac's Live Activity itself when the Mac turns busy while the app is in front; pocketd fills in the ask
    /// and keeps it current once it has the activity's token.
    nonisolated static func turnedBusy(was: Bool, now: MacActivity.ContentState) -> Bool { !was && now.isBusy }

    /// Sessions as pocketd's `push.Build` counts them: attached, and Needs you, working, Failed or Done.
    nonisolated static func sessions(_ agents: [AgentSummary]) -> [ActivitySession] {
        agents.compactMap { a in
            guard a.attached else { return nil }
            switch a.status {
            case .done: return ActivitySession(id: a.id, status: a.failed == true ? MacActivity.Top.failed : MacActivity.Top.done, updatedAt: a.updatedAt)
            case .needsYou, .working: return ActivitySession(id: a.id, status: a.status.rawValue, updatedAt: a.updatedAt)
            default: return nil
            }
        }
    }

    /// Each session's title and place as pocketd's alerts word them.
    nonisolated static func titles(_ agents: [AgentSummary]) -> [String: ActivityTitles.Entry] {
        Dictionary(agents.map { a in
            let title = a.title.isEmpty ? (["claude": "Claude Code", "codex": "Codex"][a.provider] ?? "Shell") : a.title
            let place = [a.project, a.worktree].compactMap { $0?.isEmpty == false ? $0 : nil }.joined(separator: " · ")
            return (a.id, ActivityTitles.Entry(title: title, place: a.project?.isEmpty == false ? place : "", provider: a.provider))
        }, uniquingKeysWith: { $1 })
    }

    public nonisolated static func describe(_ error: any Error) -> String {
        switch error {
        case DaemonError.server(let e): e.message
        case DaemonError.offline, DaemonError.closed: "Not connected to this Mac."
        case DaemonError.timedOut: "The Mac didn't answer."
        case RelayClient.Failure.refused(let status, let reason): "The push relay refused this phone (\(reason ?? String(status)))."
        case RelayClient.Failure.unreadable: "The push relay sent a reply this app can't read."
        default: "The push relay couldn't be reached."
        }
    }
}
