/// "claude-haiku-4-5-20251001" reads as "Haiku 4.5", "haiku" as "Haiku", "openai-codex/gpt-5.4" as "GPT 5.4",
/// "opencode/big-pickle" as "Big Pickle" and "us.anthropic.claude-opus-5-5" as "Opus 5.5": the id without who serves
/// it, its date or "claude-", numbers joined by a dot. Claude's "[1m]" for its 1M-context window stays as written.
/// The desktop's `agents::model_name` is the reference: this follows it, never the other way round.
public func modelName(_ model: String) -> String {
    let isNumber = { (word: Substring) in !word.isEmpty && word.allSatisfy { $0.isASCII && $0.isNumber } }
    var id = model.split(separator: "/", omittingEmptySubsequences: false).last ?? ""
    // Hermes writes "provider:model"; Bedrock ends an id in ":0".
    if let colon = id.lastIndex(of: ":"), !isNumber(id[id.index(after: colon)...]) { id = id[id.index(after: colon)...] }
    let window = id.firstIndex(of: "[").map { id[$0...] } ?? ""
    id = id.dropLast(window.count)
    // Bedrock leads with where and who, as "us.anthropic."; the dot of a version, as "v3.2", has no two letters after it.
    let named = { (rest: Substring) in rest.count > 1 && rest.prefix(2).allSatisfy { $0.isASCII && $0.isLetter } }
    if let dot = id.indices.last(where: { id[$0] == "." && named(id[id.index(after: $0)...]) }) { id = id[id.index(after: dot)...] }
    if id.hasPrefix("claude-") { id = id.dropFirst(7) }
    var name = ""
    var afterNumber = false
    for word in id.split(separator: "-") where !(word.count >= 8 && isNumber(word)) {
        if !name.isEmpty { name += afterNumber && isNumber(word) ? "." : " " }
        // A word without a vowel is initials, as "gpt".
        name += word.contains(where: "aeiouy".contains) ? word.prefix(1).uppercased() + word.dropFirst() : word.uppercased()
        afterNumber = isNumber(word)
    }
    return name + window
}
