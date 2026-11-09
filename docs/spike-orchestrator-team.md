# Spike: Team mode (một lead, nhiều worker) trên chat session thật

Ngày thực hiện: 2026-11-05. Code spike để ở `/tmp/spike-team` (sẽ bỏ đi), không có thay đổi nào trong repo ngoài file này.
Phiên bản đã dùng: Claude Code 2.1.318, codex-cli 0.163.0, Node v26.8.1.
Thiết kế đang kiểm: `docs/orchestrators/product/08-orchestrator-mode.md`, prototype `.ui-review/prototypes/orchestrator-team.html`.

## Mục tiêu

Kiểm xem Team mode chạy được bằng những gì pocketd đã có, chưa cần grants:

- Lead nhận yêu cầu kèm luật Team, trả về một plan dạng JSON mà pocketd parse được.
- pocketd tạo worktree cho mỗi task và chạy worker như một chat session bình thường.
- Handback của worker được gửi vào lead như một user message, và lead review chúng.
- "Đã đọc" được tính theo `command_lifecycle` của Claude: message chỉ được ack khi turn hiển thị nó kết thúc bình thường.

## Kiến trúc đã kiểm chứng

```
spike.mjs (đóng vai pocketd)
 ├─ lead:   claude -p --input-format stream-json --output-format stream-json
 │          --verbose --permission-prompt-tool stdio --model sonnet      (cwd: repo)
 │            ↑ user message: <anywhere_team>luật</anywhere_team> + yêu cầu
 │            ↓ result: giải thích + <anywhere_team_plan>{json}</anywhere_team_plan>
 ├─ validate plan → git worktree add -b team/<id> wt-<id> <base>
 ├─ worker: cùng cờ + --permission-mode acceptEdits, --model haiku   (cwd: wt-<id>)
 │            ↑ <anywhere_team_assignment>   ↓ result = summary
 │          can_use_tool → spike trả lời allow (đóng vai người dùng)
 ├─ commit worktree → gửi lead <anywhere_team_handback> (summary + diff --stat)
 └─ git merge --no-ff theo thứ tự dependency → node --test
```

Repo thử: package Node `textkit` có sẵn `slugify`. Yêu cầu: thêm `truncate` và `titleCase`, mỗi hàm có file và test riêng, export từ `src/index.js` và liệt kê trong `README.md`.

## Kết quả

| # | Giả thuyết | Kết quả |
|---|---|---|
| H1 | Lead trả về plan block hợp lệ ngay lần đầu, không sửa file | ✅ Sonnet: 3 task, mất 17s. t1 và t2 chạy song song; t3 (`src/index.js`, `README.md`) có `after: [t1, t2]`. Repo sạch sau lượt plan. Chưa cần đến lượt repair |
| H2 | Worker là chat session thường trong worktree riêng, và chỉ sửa đúng file được giao | ✅ Haiku: mỗi worker mất 10–17s, khoảng $0.008. Không worker nào sửa file ngoài `files` |
| H2b | `acceptEdits` đủ để worker không hỏi quyền | ❌ Mỗi worker hỏi 1–2 lần, đều là `Bash` để chạy test hoặc `git`. Nếu không có allowlist, mỗi worker sẽ hỏi người dùng ít nhất một lần |
| H3 | Handback gửi lúc lead đang rảnh trở thành một turn riêng | ✅ `queued → started → completed` |
| H4 | Handback gửi lúc lead đang chạy turn thì được xếp hàng, không bị mất | ✅ Handback t1 đến khi lead đang review t2. Nó `started` ngay tại tool boundary kế tiếp và nhập vào turn đang chạy: một `result` cho cả hai. Note gửi khi lead đang ở turn không dùng tool thì chạy thành turn riêng sau đó |
| H5 | Interrupt một turn chưa xong thì message không được ack | ✅ `queued → started → cancelled`, `result.subtype = error_during_execution`. Mailbox dựa vào `cancelled` để gửi lại |
| H6 | Lead nhận handback, không coi là prompt injection, và đưa ra kết luận merge | ✅ Không lần nào từ chối. Lead tự đọc `git diff main..team/t1` và kết luận **"Not ready to merge"** vì t3 thiếu export `truncate`, và kết luận này đúng (xem phát hiện 1) |
| H7 | `git merge --no-ff` theo thứ tự dependency không conflict, test pass | ✅ Merge t1 → t2 → t3 không conflict, `node --test` pass. Nhưng kết quả vẫn **sai**: `src/index.js` không export `truncate`, và test không bắt được lỗi này |
| H8 | Codex qua `codex app-server` cũng trả về plan block hợp lệ | ✅ Model mặc định của account (`gpt-6.1-sol`): cùng 3 task, cùng `after`, mất 50s, không hỏi quyền với `sandbox: read-only` |

Chi phí cả lượt Claude: $0.91. Gần hết là của lead (Sonnet), vì mỗi lượt review đọc lại toàn bộ hội thoại.

## Phát hiện

1. **Task có nhiều dependency phải chạy trên một base đã merge các dependency đó.** Spike lấy base của t3 là `team/t2`, nên t3 không thấy `src/truncate.js` và bỏ qua export đó. Worker t3 tự chạy `git ls-tree main team/t1 team/t2` để tìm file. Muốn làm đúng, pocketd phải tạo branch `team/<id>-base`, merge mọi `after` vào đó, rồi mới tạo worktree. Nếu merge này conflict thì báo cho lead.
2. **Review của lead là cổng chặn thật, test pass chưa đủ.** Ở H7, mọi branch merge sạch và test pass, nhưng lead đã nói "Not ready to merge" và nói đúng. Nút **Merge** phải hiển thị kết luận của lead, và cảnh báo nếu lead chưa nói "ready".
3. **Note của người dùng phải ghi rõ ai đã nói.** Spike gửi `You told worker t1: "…"`. Lead hiểu "You" là chính nó, nên trả lời "I never gave t1 that instruction". Dạng đúng là `The user told worker t1: "…"`.
4. **Worker đọc được branch của worker khác.** Các worktree dùng chung `.git`, nên `git ls-tree team/t1` chạy được từ worktree t3. Việc này không gây hại, nhưng phạm vi ghi (`files`) chỉ là lời dặn trong prompt, không phải sandbox. Spike này không có worker nào sửa file ngoài phạm vi.
5. **Danh sách model trong plan phải lấy theo account.** Codex với tài khoản ChatGPT từ chối `gpt-5.5` và `gpt-5.5-mini` (`model_not_found`). Luật Team phải liệt kê model mà account thật sự có, ví dụ lấy từ `model/list` của Codex, nếu không plan sẽ chọn model chạy không được.
6. **Handback có thể nhập vào turn đang chạy (H4).** Một turn có thể ack nhiều message cùng lúc. Vì vậy mailbox phải ack theo từng `command_uuid` qua `command_lifecycle`, không ack theo "turn kế tiếp kết thúc". Claude đã cung cấp đúng thứ này.
7. **Không cần hook hay socket để đánh thức lead.** Với chat mode, mailbox chỉ cần ghi một dòng stream-json vào stdin. Claude tự xếp hàng và báo trạng thái.

## Ảnh hưởng đến thiết kế

- **MVP không cần grants.** Lead chỉ trả về plan block, và pocketd làm mọi việc còn lại: tạo worktree, launch worker, gửi handback, merge. Thứ tự build trong doc 08 đổi thành: `LaunchSpec.Parent` + sub-session → mailbox → Team → grants (E15).
- **Mailbox cho Claude chat** = hàng đợi bền trong pocketd + `command_lifecycle`. `completed` thì ack; `cancelled`, process chết, hoặc không có `completed` thì gửi lại. Phía Codex chưa kiểm (xem bên dưới).
- **Team cần thêm:** branch base cho task nhiều dependency (phát hiện 1), danh sách model theo account (phát hiện 5), và mẫu note ghi rõ người nói (phát hiện 3).
- **Câu hỏi mở về allowlist trong doc 08 trở nên cấp bách hơn:** theo H2b, plan 4 worker sẽ hỏi người dùng ít nhất 4 lần chỉ để chạy test.

## Chưa kiểm chứng

- Mailbox phía Codex: `turn/start` khi thread đang có turn chạy thì xử lý thế nào, và trạng thái `interrupted` của turn có tương đương `cancelled` của Claude không.
- Worker Codex chạy thật trong worktree (spike chỉ dùng Codex để lập plan).
- Plan sai cần lượt repair: cả Sonnet và Codex đều đúng ngay lần đầu, nên đường repair chưa chạy.
- Merge conflict giữa các worker, và base branch cho task nhiều dependency (phát hiện 1).
- Handback lớn hơn khoảng 9.000 ký tự.
- Chặn phạm vi ghi bằng `PreToolUse`.
