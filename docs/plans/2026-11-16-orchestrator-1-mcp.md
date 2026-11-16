# Orchestrator P1: MCP Endpoint

**Goal:** Every Claude, Codex and HTTP-MCP ACP chat lists Anywhere's tools. Each call is authenticated as its session.

**Read first:**
- `docs/plans/2026-11-16-orchestrator.md`: Decisions → MCP.
- T3 `f8c8f30e`: `docs/orchestration-v2/orchestrator-mcp-server.md` ("Transport And Authentication", "Provider Injection"), `apps/server/src/mcp/McpSessionRegistry.ts`, `McpToolExposure.ts`, `McpToolAccess.ts`.
- `packages/pocketd/cmd/pocketd/serve.go:288`: the owner TCP listener, the pattern for a second loopback listener.
- `packages/pocketd/internal/launch/claude.go:31` (`chatFlags`), `internal/daemon/codex_chat.go:206` (`thread/start` / `thread/resume`), `internal/acp/session.go:289`, `:313`, `:1142` (`mcpServers` on `session/new` and `session/load`).
- `packages/pocketd/internal/handoff/handoff.go:27`: `Terminal.Args` is written to `handoff.json`, so no token on argv.
- `packages/pocketd/internal/peer/scope.go`, `check.go`: scopes. A token is a new principal kind.

---

## PR 1.1: Endpoint spike and server

- A one-day spike: hand-rolled streamable HTTP (`initialize`, `tools/list`, `tools/call`, `notifications/tools/list_changed`, `mcp-protocol-version` header) versus `github.com/modelcontextprotocol/go-sdk`. Pick by size and conformance against Claude, Codex and one ACP agent. Record the pick in the index's Open questions.
- `internal/mcp/server.go`: listens on `127.0.0.1:<port>`. The port is saved in config on first start (`config.MCPPort`), and a taken port fails with a clear log line. The listener is passed on in-place upgrade like the owner listener.
- Tests: `initialize` and `tools/list` round-trip; a missing or bad bearer gets 401.

## PR 1.2: Tokens

- `internal/mcp/tokens.go`: `Issue(sessionID, agent)` returns the token; the store keeps its SHA-256. `Resolve` touches `last_alive`; idle over 24 h marks it expired (the larger of wall and monotonic elapsed, as T3). `Revoke(session)` runs on `session.closed`.
- The principal `mcp` has scope `Delegate`. It is never `Approve`, and the WebSocket and ops servers don't accept it.
- Tests: hash only on disk; idle expiry; revoke on close; a token survives a pocketd restart; a token can't call a WebSocket verb.

## PR 1.3: Injection

- Claude: `--mcp-config state/mcp/<session>.json` (`0600`, removed on close) and `--allowedTools mcp__anywhere__*`, added by `launch` when the session is a chat. The spike first checks whether a stream-json control request can set MCP servers over stdin instead. Logs say only whether MCP config exists.
- Codex chat: `anywhere` and `anywhere-direct` in `thread/start` and `thread/resume` config. A Codex fork runs in the TUI and gets none.
- ACP: an HTTP `mcpServers` entry on `session/new` and `session/load` when the agent's `initialize` advertises HTTP MCP.
- Tokens are issued at launch and restore. A chat restored after a restart gets a fresh token.
- Tests: the e2e fakes record the config they were given; each one lists the tools; no token appears in any argv, `handoff.json` or log.

## PR 1.4: Exposure

- `internal/mcp/exposure.go`: four direct tools plus `tool_search` (term scoring as T3, at most 10 results). Loaded tools stay listed for the session and announce `list_changed`.
- Tool definitions live in `internal/mcp/tools/`, one file per group. P1 registers `orchestrator_capabilities` only; the rest land in P2–P4.
- Tests: `tool_search "queue"` lists `anywhere_queue` and it stays listed; `anywhere-direct` lists only the two blocking tools.
