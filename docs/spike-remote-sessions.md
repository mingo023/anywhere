# Spike: điều khiển Claude Code và Codex trên Mac từ điện thoại

Ngày thực hiện: 2026-09-23. Code spike để ở `/tmp/spike` (sẽ bỏ đi), không có thay đổi nào trong repo.
Phiên bản đã dùng: Claude Code 2.1.280, Codex 0.156.1, ghostty commit `4ae9f1a`, Zig 0.16.0, Rust 1.98.1, gpui-kit 0.6.6.

## Mục tiêu

- Session Claude/Codex chạy trên MacBook được mirror lên điện thoại và điều khiển được từ điện thoại.
- Dùng được nhiều account. Người dùng tự chọn account bằng env (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`); pocket không quản lý account.

## Kiến trúc đã kiểm chứng

```
terminal ─ pocketd run claude ─┐
GPUI desktop (Rust + libghostty)┼─ unix socket, JSON lines ─► pocketd (Go)
phone (chat UI) ───────────────┘                               ├─ PTY + libghostty-vt (cgo)
                                                               ├─ tail transcript → timeline
                                                               ├─ Claude: hook PermissionRequest
                                                               └─ Codex: app-server JSON-RPC
```

- **pocketd (Go):**
  - Giữ PTY của từng session, và mỗi session có một libghostty-vt headless.
  - Session vẫn sống khi client đóng.
  - Khi client attach, gửi snapshot VT rồi stream các byte tiếp theo.
- **Desktop:** gpui-kit, vai trò Dashboard + host, tham khảo MonoCode (mỗi tab là một session, có composer để nhập). Render bằng libghostty-vt qua C shim.
- **Phone:** giữ nguyên chat UI và protocol hiện tại. Phone không cần biết gì về terminal.

## Kết quả

### Hạ tầng

| Hạng mục | Kết quả |
|---|---|
| Build libghostty-vt: `zig build -Demit-lib-vt -Doptimize=ReleaseFast` | ✅ mất 1m27s, ra `.a`, `.dylib`, xcframework (macOS và iOS), header `ghostty/vt.h` |
| Go gọi libghostty qua cgo, link static | ✅ binary 9.9MB |
| Session sống khi client thoát, attach lại với size khác thì vẽ lại đúng | ✅ |
| Codex chạy không cần terminal thật: daemon tự trả lời query terminal qua callback `WRITE_PTY` | ✅ |
| GPUI render terminal (màu, bold, cursor, ký tự kẻ khung) và tự cập nhật live | ✅ |
| Gửi prompt và approve bằng phím qua socket, cho cả Claude và Codex | ✅ |

### Transcript (nguồn cho `agent.timeline` / `agent.update`)

| | Claude | Codex |
|---|---|---|
| File | `~/.claude/projects/<slug>/<session-id>.jsonl`. Biết đường dẫn qua `--session-id` hoặc hook `SessionStart` | `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl`. Match theo `session_meta` (`id`, `cwd`, `timestamp`) |
| Độ trễ so với màn hình | khoảng 0.1s | khoảng 0 đến 0.1s |
| Format | Giống SDK, tái dùng được `mapMessage` | Cần mapper mới (`response_item`, `event_msg`, …) |
| Ghi approval đang chờ | ❌ Không. Tool call được ghi trước khi dialog hiện | ❌ Không. `custom_tool_call` được ghi trước khi dialog hiện |
| Token streaming | ❌ Chỉ ghi message khi đã hoàn chỉnh | ❌ Qua rollout thì không. App-server có `item/agentMessage/delta` |

### Approval (`permission.request` / `permission.resolve`)

**Claude: hook `PermissionRequest`**, inject lúc spawn bằng `--settings`.

- Payload hook nhận được: `tool_name`, `tool_input`, `permission_suggestions`.
- Hook trả về:
  - Cho phép: `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`
  - Từ chối: `{"behavior":"deny","message":"..."}`. Model nhận được `message`.
- Hook chạy song song với dialog trên desktop. Hook trả lời thì dialog tự đóng.
- Nếu desktop bấm trước, lệnh vẫn chạy nhưng **process hook không bị kill**. Hook trả lời muộn sau đó thì bị bỏ qua, không gây lỗi.
  - Hệ quả: daemon phải tự nhận biết approval đã xong (có `tool_result` trong transcript hoặc dialog biến khỏi màn hình), rồi thả hook ra và gỡ card trên phone.

**Codex: app-server.** TUI chạy `codex --remote unix://$CODEX_HOME/app-server-control/app-server-control.sock -C <cwd>`.

- Cần chạy `codex app-server daemon start` trước.
- Socket là WebSocket qua unix socket; phải tắt `perMessageDeflate` mới kết nối được.
- pocketd làm client thứ hai:
  - Gọi `thread/loaded/list` rồi `thread/resume` để subscribe.
  - Phải retry vì resume lỗi cho tới khi thread có turn đầu tiên.
- Approval đến dưới dạng server request `item/commandExecution/requestApproval`, gồm `command`, `reason`, `cwd`, `availableDecisions`. Kèm theo cờ `waitingOnApproval` trong `thread/status/changed`.
- Phone trả lời `{decision:"accept"}` thì dialog trên TUI tự đóng và lệnh chạy.
- Nếu desktop bấm trước, client nhận `serverRequest/resolved`. Trả lời muộn thì bị bỏ qua.
- Ngoài approval còn có:
  - `turn/start`: gửi prompt từ phone, TUI hiển thị.
  - `item/*` và `turn/*`: stream theo thời gian thực, có cả token delta.

### Các hướng đã loại

- **Claude Channels (MCP `claude/channel`):** bị skip với lỗi "not on the approved channels allowlist". Cờ `--dangerously-load-development-channels` cần xác nhận dialog lúc khởi động. Message đi qua channel có `skipSlashCommands`. Đây là research preview.
- **Parse màn hình để lấy chi tiết permission:** không cần nữa, vì đã có hook (Claude) và app-server (Codex).
- **Node + node-pty + ghostty-web WASM:** vẫn chạy được, nhưng thay bằng Go + cgo theo lựa chọn stack.

## Mapping protocol hiện tại

| Protocol | pocketd |
|---|---|
| `agent.list` | Danh sách session PTY |
| `agent.create` | Spawn CLI trong PTY |
| `agent.timeline` / `agent.update` | Claude: tail JSONL. Codex: event app-server (hoặc rollout) |
| `agent.stream` | Codex: `item/agentMessage/delta`. Claude: không có |
| `agent.prompt` | Claude: ghi text, đợi khoảng 150ms rồi gửi `\r`. Codex: `turn/start` (hoặc ghi vào PTY) |
| `agent.interrupt` | `Esc` |
| `agent.compact` | `/compact\r` |
| `agent.close` | Kill PTY |
| `permission.request` / `resolve` | Claude: hook. Codex: server request |
| `profile.list` | Bỏ |
| `task.stop` | Chưa có cách tương đương |

## Những điểm phải nhớ khi làm thật

- **Env khi spawn Claude phải có `USER`.** Thiếu thì báo "Not logged in" vì keychain lookup cần biến này.
- **Claude trên máy này mặc định chạy auto mode.** Muốn có dialog duyệt quyền thì chạy `--permission-mode default`. Codex thì dùng `-s read-only -a on-request`.
- **Hooks chỉ chạy trong thư mục đã được trust.**
- **Codex `--remote` nghĩa là session chạy trong app-server daemon**, TUI chỉ là client.
  - Mỗi `CODEX_HOME` sẽ có daemon riêng (chưa test đa account).
  - Protocol thay đổi theo version; daemon từng tự update trong lúc spike.
- **Kích thước PTY:** client nào resize sau cùng thì PTY theo size đó.
- **Rust:** thư mục chứa lib chỉ được có `.a`. Nếu có cả `.dylib`, linker sẽ lấy dylib.
- **gpui-kit 0.6.6:** dùng `cx.open_window` + `Root::new`; `window.focus(&h, cx)`.
- **libghostty C API chưa có version ổn định.** Cần pin commit.

## Chưa kiểm chứng

- Gõ phím trực tiếp trong app GPUI (cần quyền Accessibility để giả lập phím).
- Đa account thật: nhiều `CLAUDE_CONFIG_DIR`, nhiều `CODEX_HOME` / daemon.
- Codex: event `thread/closed` xuất hiện trong lúc approval đang treo khoảng 40s, nhưng các event sau đó vẫn tới bình thường. Chưa rõ nguyên nhân.
- Khoảng delay 150ms trước `\r` khi gửi prompt là chọn theo kinh nghiệm, chưa đo.
- Hook Claude khi bị treo (desktop đã duyệt) giữ process tới khi timeout; chưa đo ảnh hưởng.
