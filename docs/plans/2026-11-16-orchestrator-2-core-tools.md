# Orchestrator P2: Core Tools and Desktop

**Goal:** Claude delegates to Codex and gets the result. Async completion wakes the parent. A child's question reaches the parent. The desktop shows delegated sessions in the panel, the delivery rows and the child view.

**Read first:**
- `docs/plans/2026-11-16-orchestrator.md`: Decisions → Delegation, Desktop; Contracts → Tools.
- `.ui-review/prototypes/delegated-panel.html`: the panel, delivery rows and child view.
- `packages/desktop/crates/pocket/src/chat/sub_agent.rs`, `tool_row.rs`: the voice of the delivery rows.

---

## PR 2.1: Delegation in the engine

- Commands `delegation.request`, `delegation.cancel` and `delegation.wake_policy`.
- `decide` enforces, in order:
  1. An active parent turn owned by the caller.
  2. Depth ≤ 3.
  3. Access equals the parent's, with step-down or refusal as the index says.
  4. Per-session (6) and per-Mac (12) concurrency. Over a limit, the child queues.
- Effects: `worktree.create` (from the parent's HEAD commit; a dirty parent is reported) and `agent.launch` with the brief as the only prompt.
- Reactors:
  - Finalize on the child's terminal turn: summary from the latest assistant item, or the error.
  - Wake: steer or queue as the index says; merge siblings into one wake; on Stop or cancel, cancel `wake` queue entries for good.
  - Question routing: queue the notice, cancel it on answer or settle.
- Tests: one `decide` table test per rule; e2e runs fake Claude → fake Codex through completion, a wait that times out then wakes, a batch of two, Stop disposes, and a question notice cancelled by an earlier answer.

## PR 2.2: Group A tools

`orchestrator_capabilities`, `delegate_task`, `task_status`, `task_cancel` (subtree depth first), `create_threads`, `anywhere_thread_launch`, `anywhere_thread_list`, `anywhere_thread_search`, `anywhere_thread_read`, `anywhere_thread_send`, `anywhere_thread_wait`, `anywhere_thread_interrupt`, `anywhere_thread_configure`, `anywhere_pending_request` (questions only), `anywhere_queue`.

- Each tool is a thin translation to a command or a projection read, with the ceiling on cross-session mutations.
- `clientRequestId` derives the command id within the token's session.
- Every wait returns within 45 s.
- Results carry `link: anywhere://session/<id>`.
- Tests: the integration test covers each tool's cases, with fakes as the only substitutes.

## PR 2.3: Wire and alerts

- `AgentSummary` lineage fields, `agent.delegations`, `delegation.update`, and the `delivery` timeline item, in Go, TS and Swift goldens.
- `push` and Up next: a child's Done neither alerts nor enters Up next; its Needs you does.
- Tests: goldens; a push test for a child's Done.

## PR 2.4: Desktop

- A model crate type `Delegations` in `agents`: plain data plus status logic, tested without GPUI.
- `pocket/src/delegated.rs` with `delegated/`:
  - `panel.rs`: capsule and list, springs, auto open and collapse, collapse on scroll.
  - `delivery.rs`: the rows. Copy is `done`, `failed`, `asks`, which pass `tests/avoid_list.rs`.
  - `child_view.rs`: push transition and breadcrumb.
- `chat.rs` wires it; `Desktop::new` only composes (ADR 0003).
- `anywhere://session/<id>` opens a session.
- Screens: before and after with `.ui-review/fixture/capture.sh` (new fixture steps for a parent with three children); `verify-anywhere` for click → child → Esc.
