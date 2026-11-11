import Testing
@testable import Domain

@Suite("Model names")
struct ModelNameTests {
    @Test("a model id reads as its family and version")
    func familyAndVersion() {
        #expect(modelName("claude-opus-5-5") == "Opus 5.5")
        #expect(modelName("claude-haiku-4-5-20251001") == "Haiku 4.5")
        #expect(modelName("haiku") == "Haiku")
        #expect(modelName("gpt-5.5") == "GPT 5.5")
    }

    @Test("a model reads without who serves it, its date or its dashes, as on the desktop")
    func servedModelName() {
        let ids = [
            "openai-codex/gpt-5.4", "opencode/big-pickle", "anthropic/claude-sonnet-4-5", "openai-codex:gpt-6-sol", "zai/glm-5.2-fast",
            "composer-2.5", "gemini-3.8-flash-high", "claude-3-5-sonnet-20241022", "deepseek/deepseek-v4-pro", "haiku", "",
        ]
        #expect(ids.map(modelName) == [
            "GPT 5.4", "Big Pickle", "Sonnet 4.5", "GPT 6 Sol", "GLM 5.2 Fast", "Composer 2.5", "Gemini 3.8 Flash High", "3.5 Sonnet",
            "Deepseek V4 Pro", "Haiku", "",
        ])
    }

    @Test("Claude's 1M-context mark stays as written after the name")
    func contextWindow() {
        let ids = ["claude-opus-4-7[1m]", "sonnet[1m]", "claude-haiku-4-5-20251001[1m]", "[1m]"]
        #expect(ids.map(modelName) == ["Opus 4.7[1m]", "Sonnet[1m]", "Haiku 4.5[1m]", "[1m]"])
    }

    @Test("a Bedrock id reads without where and who serve it")
    func bedrock() {
        let ids = [
            "amazon-bedrock/us.anthropic.claude-opus-5-5", "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
            "amazon-bedrock/anthropic.claude-haiku-4-5-20251001-v1:0", "amazon-bedrock/amazon.nova-lite-v1:0",
            "amazon-bedrock/us-gov.openai.gpt-oss-120b-1:0", "amazon-bedrock/global.openai.gpt-6.1-sol", "amazon-bedrock/moonshotai.kimi-k2.5",
            "amazon-bedrock/qwen.qwen3-32b-v1:0", "amazon-bedrock/minimax.minimax-m2.5",
        ]
        #expect(ids.map(modelName) == [
            "Opus 5.5", "Sonnet 4.5 V1:0", "Haiku 4.5 V1:0", "Nova Lite V1:0", "GPT Oss 120B 1:0", "GPT 6.1 Sol", "Kimi K2.5", "Qwen3 32B V1:0",
            "Minimax M2.5",
        ])
    }

    @Test("a version's dot is no vendor prefix")
    func versionDot() {
        let ids = ["gpt-5.4", "qwen3.5-coder", "MiniMax-M2.5", "amazon-bedrock/deepseek.v3.2", "opencode/mimo-v2.6-flash-free", "a.b"]
        #expect(ids.map(modelName) == ["GPT 5.4", "Qwen3.5 Coder", "MiniMax M2.5", "Deepseek.v3.2", "Mimo V2.6 Flash Free", "A.b"])
    }
}
