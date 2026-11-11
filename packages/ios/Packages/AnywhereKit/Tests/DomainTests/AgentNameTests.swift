import Testing
@testable import Domain

@Suite("Agent names")
struct AgentNameTests {
    @Test("each agent names effort its own way, one this build doesn't know as Effort")
    func effort() {
        let ids = ["claude", "codex", "cursor", "grok", "opencode", "pi", "omp", "fx", "hermes", "antigravity", "gemini"]
        #expect(ids.map(AgentName.effort) == [
            "Effort", "Reasoning", "Effort", "Reasoning", "Variant", "Thinking", "Thinking", "Effort", "Effort", "Effort", "Effort",
        ])
    }

    @Test("an agent this build doesn't know goes by its id with a capital, the rest of it as written")
    func label() {
        #expect(["hermes", "omp", "gemini", "openHands", ""].map(AgentName.label) == ["Hermes", "omp", "Gemini", "OpenHands", ""])
        #expect(AgentName.short("gemini") == nil && AgentName.full("gemini") == nil)
    }
}
