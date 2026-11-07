import Foundation

/// Which projects the home list shows open. A project the user opened or closed stays so until they change it; one
/// they never touched is open while any of its sessions is working or wants a look.
struct Folds: RawRepresentable, Hashable {
    var chosen: [String: Bool] = [:]

    init() {}

    init?(rawValue: String) {
        guard let chosen = try? JSONDecoder().decode([String: Bool].self, from: Data(rawValue.utf8)) else { return nil }
        self.chosen = chosen
    }

    /// Sorted, so the same folds always store the same text: RawRepresentable compares by it.
    var rawValue: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        return (try? encoder.encode(chosen)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }

    /// `lively`: a session in the project is working or wants a look.
    func isOpen(_ project: String, lively: Bool) -> Bool {
        chosen[project] ?? lively
    }

    mutating func toggle(_ project: String, lively: Bool) {
        chosen[project] = !isOpen(project, lively: lively)
    }
}
