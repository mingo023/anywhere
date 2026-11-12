window.SEED = () => ({
  tickets: [
    { id: 'ANY-47', station: 'todo', title: 'Remember the last-picked agent per project in the new-session composer', label: 'Feature', agent: 'C', mins: 120, pri: 2, project: 'anywhere' },
    { id: 'ANY-48', station: 'todo', title: 'Docs: explain Locals vs worktrees on the website', label: 'Docs', agent: 'C', mins: 1440, pri: 1, project: 'pocket-site' },
    { id: 'ANY-50', station: 'todo', title: 'Crash when a worktree path contains a space', label: 'Bug', agent: 'X', mins: 35, pri: 4, project: 'anywhere' },
    { id: 'ANY-45', station: 'todo', title: 'Keyboard shortcut to jump to the next ticket that needs you', label: 'Feature', agent: 'C', mins: 300, pri: 2, project: 'anywhere' },
    { id: 'ANY-46', station: 'spec', title: 'Show the ticket key in the session sidebar', label: 'Feature', agent: 'C', mins: 4, pri: 2, project: 'anywhere', work: 'Drafting' },
    { id: 'ANY-49', station: 'spec', title: 'Warn before closing a window with running workers', label: 'Feature', agent: 'C', mins: 9, pri: 2, project: 'anywhere', work: 'Drafting' },
    { id: 'ANY-44', station: 'plan', gate: 'plan', title: 'Let each project set its own Board slot limit', label: 'Feature', agent: 'C', mins: 12, pri: 3, project: 'anywhere' },
    { id: 'ANY-51', station: 'plan', title: 'Show allowlist hits per worker in the Workers tab', label: 'Feature', agent: 'C', mins: 6, pri: 1, project: 'anywhere', work: 'Planning' },
    { id: 'ANY-43', station: 'implement', title: 'Phone: answer an ask from the lock screen', label: 'Feature', agent: 'C', mins: 31, pri: 3, project: 'anywhere', work: '1/3', branch: 'team-3fa91c', slot: true },
    { id: 'ANY-42', station: 'implement', gate: 'ask', title: 'Retry the pocketd link with backoff', label: 'Bug', agent: 'X', mins: 44, pri: 4, project: 'anywhere', work: '1/2', branch: 'team-7c2e10', ask: 'cargo test -p daemon' },
    { id: 'ANY-40', station: 'checks', title: 'Group tickets by label in the list layout', label: 'Feature', agent: 'C', mins: 60, pri: 2, project: 'anywhere', work: '2/4 checks', branch: 'team-5d0e21', slot: true },
    { id: 'ANY-39', station: 'checks', failed: true, title: 'Fold unchanged hunks in the diff', label: 'Feature', agent: 'C', mins: 120, pri: 2, project: 'anywhere', branch: 'team-a41c07' },
    { id: 'ANY-41', station: 'checks', title: 'Debounce file watcher events on large repos', label: 'Feature', agent: 'X', mins: 22, pri: 2, project: 'anywhere', work: '1/4 checks', branch: 'team-2e7b90', slot: true },
    { id: 'ANY-38', station: 'pr', gate: 'pr', title: 'Show a team’s merge conflicts file by file in the capsule', label: 'Feature', agent: 'C', mins: 18, pri: 3, project: 'anywhere', branch: 'team-91b4d2', pr: 203, diff: [214, 37] },
    { id: 'ANY-37', station: 'pr', title: 'Copy the branch name from the ticket header', label: 'Feature', agent: 'C', mins: 9, pri: 1, project: 'anywhere', work: 'Reviewing', pr: 202 },
    { id: 'ANY-36', station: 'merged', title: 'Paste images from the clipboard into the composer', label: 'Feature', agent: 'C', mins: 180, pri: 2, project: 'anywhere', pr: 201, diff: [96, 4] },
    { id: 'ANY-35', station: 'merged', title: 'Phone pairing QR refreshes itself', label: 'Bug', agent: 'X', mins: 300, pri: 2, project: 'anywhere', pr: 200, diff: [58, 21] },
    { id: 'ANY-34', station: 'merged', title: 'Keep the scroll position when switching sessions', label: 'Bug', agent: 'C', mins: 360, pri: 3, project: 'anywhere', pr: 199, diff: [33, 9] },
    { id: 'ANY-33', station: 'merged', title: 'pocketd: rotate logs at 50 MB', label: 'Infra', agent: 'X', mins: 1440, pri: 1, project: 'anywhere', pr: 198, diff: [74, 2] },
  ],

  details: {
    'ANY-44': {
      created: 'Today, 14:08',
      spec: {
        by: 'Spec · Claude · 14:13',
        brief: 'Today every project shares one Board slot limit, so a busy project can starve a quiet one. The limit moves into the project record and the scheduler counts running tickets per project. The global setting stays as the default for projects that never set their own value. The wire format changes in pocketd, so the TypeScript and Swift mirrors ship in the same PR.',
        criteria: [
          'A project can override the Board limit (1–8) from its settings',
          'Projects without an override keep using the global limit',
          'The scheduler counts slots per project, not across all projects',
          'The limit chip on the Board header shows busy/limit for the filtered project',
          'Existing board.db files load without migration errors',
        ],
      },
      tasks: [
        { id: 't1', title: 'Store and enforce a per-project limit in pocketd', body: 'Add a per-project limit map to the store, read it when pumping the queue, keep the global default as fallback. Test: scheduler honours per-project limits.', files: ['board/store.go', 'board/scheduler.go', 'proto/ticket.go'], agent: 'Go backend', allow: 'go test ./...' },
        { id: 't2', title: 'Mirror the wire change in TypeScript and Swift', body: 'Wire changes land in Go, TS and Swift in the same PR. Golden tests must decode the new field.', files: ['src/board.ts', 'Wire/Board.swift'], agent: 'Codex', after: 't1', allow: 'pnpm -r typecheck, swift test' },
        { id: 't3', title: 'Desktop picker and header chip', body: 'Add a 1–8 picker to project settings and show busy/limit for the filtered project in the Board header.', files: ['board/top_bar.rs', 'settings/project.rs'], agent: 'Claude · Sonnet', after: 't1', allow: 'cargo test -p pocket' },
      ],
      activity: [
        { s: 'To do', who: 'You', av: 'you', text: 'Created the ticket and added the label Feature', time: '16m' },
        { s: 'To do', who: 'pocketd', av: 'sys', text: 'Started the ticket. Slot reserved, 2 of 4 busy', time: '16m' },
        { s: 'Spec', who: 'Lead · Claude', av: 'C', text: 'Read board/store.go, scheduler.go, proto/ticket.go and board/top_bar.rs. The limit lives in one place and the desktop only displays it', time: '15m' },
        { s: 'Spec', who: 'Lead · Claude', av: 'C', text: 'Wrote the spec with 5 acceptance criteria. The wire change needs Go, TS and Swift mirrors in one PR', time: '11m' },
        { s: 'Spec', who: 'You', av: 'you', text: 'commented', time: '9m', quote: 'Keep the global limit as the fallback. Old boards should load without a migration step.' },
        { s: 'Plan', who: 'Lead · Claude', av: 'C', text: 'Proposed a plan with 3 tasks. t2 and t3 start after t1. Allowlists cover tests only', time: '4m' },
        { s: 'Plan', who: 'Gate', av: 'sys', text: 'Waiting for you to approve the plan', time: '4m' },
      ],
    },

    'ANY-43': {
      created: 'Today, 14:01',
      spec: { by: 'Spec · Claude · 14:04', brief: 'The lock-screen action needs a notification category with Answer and Dismiss, handled while the app is not running. The payload comes from pocketd, so the Go, TypeScript and Swift mirrors change together.' },
      tasks: [
        { id: 't1', title: 'Notification action payload in pocketd (Go)', agent: 'Claude · Sonnet', branch: 'team-3fa91c-t1', diff: [88, 6], state: 'Done' },
        { id: 't2', title: 'Wire mirror in TypeScript', agent: 'Codex', branch: 'team-3fa91c-t2', diff: [41, 0], state: 'Working', note: 'Updating packages/protocol/src/push.ts and the golden fixtures' },
        { id: 't3', title: 'Wire mirror and action handler in Swift', agent: 'Claude · Sonnet', branch: 'team-3fa91c-t3', diff: [120, 12], state: 'Working', note: 'Adding the Answer action to the notification category in PushHandler.swift' },
      ],
      activity: [
        { s: 'To do', who: 'You', av: 'you', text: 'Started the ticket', time: '23m' },
        { s: 'Spec', who: 'Lead · Claude', av: 'C', text: 'Wrote the spec: a notification category with Answer and Dismiss, handled while the app is not running', time: '20m' },
        { s: 'Plan', who: 'You', av: 'you', text: 'Approved the plan (3 tasks, 2 agents). Removed no tasks', time: '18m' },
        { s: 'Implement', who: 'Lead · Claude', av: 'C', text: 'Started t1. t2 and t3 wait for its branch', time: '18m' },
        { s: 'Implement', who: 'pocketd', av: 'sys', text: 'Auto-allowed go test ./... for t1, it matches the allowlist', time: '13m' },
        { s: 'Implement', who: 'Worker t1 · Claude', av: 'C', text: 'Handed back: action payload added to push, tests pass', time: '7m', diff: 'push/payload.go       | 61 ++++++\npush/payload_test.go  | 27 +++++\nproto/push.go         |  6 +-\n3 files changed, 88 insertions(+), 6 deletions(-)' },
        { s: 'Implement', who: 'Lead · Claude', av: 'C', text: 'Started t2 (Codex) and t3 (Claude) from t1’s branch, in parallel', time: '7m' },
      ],
    },

    'ANY-42': {
      created: 'Today, 13:38',
      spec: { by: 'Spec · Claude · 13:42', brief: 'Reconnect to pocketd with exponential backoff, 0.5 s to 30 s with jitter. Show a banner after 3 failures.' },
      tasks: [
        { id: 't1', title: 'Backoff in the daemon client', agent: 'Codex', branch: 'team-7c2e10-t1', diff: [64, 9], state: 'Needs you', ask: 'cargo test -p daemon' },
        { id: 't2', title: 'Reconnect banner copy', agent: 'Claude · Sonnet', branch: 'team-7c2e10-t2', diff: [12, 3], state: 'Done' },
      ],
      activity: [
        { s: 'To do', who: 'You', av: 'you', text: 'Created the ticket and added the label Bug', time: '46m' },
        { s: 'Spec', who: 'Lead · Claude', av: 'C', text: 'Wrote the spec: reconnect with exponential backoff, show a banner after 3 failures', time: '42m' },
        { s: 'Plan', who: 'You', av: 'you', text: 'Approved the plan (2 tasks, 2 agents)', time: '37m' },
        { s: 'Implement', who: 'Lead · Claude', av: 'C', text: 'Started t1 (Codex) and t2 (Claude) in parallel. No dependencies', time: '37m' },
        { s: 'Implement', who: 'Worker t2 · Claude', av: 'C', text: 'Handed back: reconnect banner copy and its snapshot test', time: '22m', diff: 'ui/reconnect_banner.rs | 15 ++++++---\n1 file changed, 12 insertions(+), 3 deletions(-)' },
        { s: 'Implement', who: 'Worker t1 · Codex', av: 'X', text: 'Asks to run cargo test -p daemon. Its allowlist has cargo check -p daemon', time: '4m' },
      ],
    },

    'ANY-39': {
      created: 'Today, 11:58',
      spec: { by: 'Spec · Claude · 12:03', brief: 'Fold runs of 8 or more unchanged lines in the diff. Expand on click and remember the choice per file.' },
      tasks: [
        { id: 't1', title: 'Fold unchanged hunks', agent: 'Claude · Sonnet', branch: 'team-a41c07-t1', diff: [168, 12], state: 'Done' },
      ],
      checks: [
        { name: 'fmt', cmd: 'cargo fmt --check', time: '2s', ok: true },
        { name: 'clippy', cmd: 'cargo clippy --workspace --all-targets', time: '48s', ok: false },
        { name: 'tests · desktop', cmd: 'cargo test --workspace', time: '1m 52s', ok: true },
        { name: 'tests · pocketd', cmd: 'go test ./...', time: '21s', ok: true },
      ],
      failure: {
        title: 'clippy · too_many_arguments',
        attempts: 'Auto-fix 2 of 2 used',
        output: 'error: this function has too many arguments (8/7)\n  --> desktop/src/diff/fold.rs:88:1\n   |\n88 | pub fn fold_hunks(\n   | ^^^^^^^^^^^^^^^^^\n   = note: `-D clippy::too-many-arguments` implied by `-D warnings`',
      },
      activity: [
        { s: 'To do', who: 'You', av: 'you', text: 'Started the ticket', time: '2h' },
        { s: 'Spec', who: 'Lead · Claude', av: 'C', text: 'Wrote the spec: fold runs of 8+ unchanged lines, expand on click, remember per file', time: '2h' },
        { s: 'Plan', who: 'You', av: 'you', text: 'Approved the plan (1 task)', time: '2h' },
        { s: 'Implement', who: 'Worker t1 · Claude', av: 'C', text: 'Handed back: folding in the diff view', time: '36m', diff: 'desktop/src/diff/fold.rs | 142 +++++++++++\ndesktop/src/diff/view.rs |  38 ++++--\n2 files changed, 168 insertions(+), 12 deletions(-)' },
        { s: 'Checks', who: 'pocketd', av: 'sys', text: 'cargo clippy failed: too_many_arguments in diff/fold.rs:88', time: '32m' },
        { s: 'Checks', who: 'Lead · Claude', av: 'C', text: 'Auto-fix 1 of 2: moved two flags into a struct. clippy still fails, 8 of 7 arguments', time: '28m' },
        { s: 'Checks', who: 'Lead · Claude', av: 'C', text: 'Auto-fix 2 of 2: inlined a helper. A second call site still passes 8 arguments', time: '23m' },
        { s: 'Checks', who: 'pocketd', av: 'sys', text: 'Checks failed after 2 auto-fixes. Waiting for you', time: '22m' },
      ],
    },

    'ANY-38': {
      created: 'Today, 13:12',
      spec: { by: 'Spec · Claude · 13:16', brief: 'List conflicted files in the capsule, one row each, with ours and theirs counts. The lead resolves files one at a time, so the order must stay stable.' },
      tasks: [
        { id: 't1', title: 'Group conflicts per file', agent: 'Claude · Sonnet', branch: 'team-91b4d2-t1', diff: [96, 20], state: 'Done' },
        { id: 't2', title: 'Capsule rows', agent: 'Claude · Sonnet', branch: 'team-91b4d2-t2', diff: [118, 17], state: 'Done' },
      ],
      checks: [
        { name: 'fmt', cmd: 'cargo fmt --check', time: '2s', ok: true },
        { name: 'clippy', cmd: 'cargo clippy --workspace --all-targets', time: '51s', ok: true },
        { name: 'tests · desktop', cmd: 'cargo test --workspace', time: '1m 47s', ok: true },
        { name: 'tests · pocketd', cmd: 'go test ./...', time: '20s', ok: true },
      ],
      prInfo: { from: 'team-91b4d2', into: 'main', files: 6 },
      gateRows: [['Checks passed', '4/4'], ['No conflicts with main', 'Clean'], ['Reviewer', '0 critical · 2 minor'], ['Your review', 'Not yet']],
      findings: [
        { sev: 'minor', text: 'The conflict list re-sorts on every poll; keep file order stable.', at: 'capsule/conflicts.rs:142' },
        { sev: 'minor', text: 'No empty state after the lead resolves the last conflict.', at: 'capsule/view.rs:67' },
        { sev: 'nit', text: 'Rename cf to conflict.', at: 'capsule/conflicts.rs:31' },
      ],
      activity: [
        { s: 'To do', who: 'You', av: 'you', text: 'Created the ticket and added the label Feature', time: '1h' },
        { s: 'Spec', who: 'Lead · Claude', av: 'C', text: 'Wrote the spec: list conflicted files in the capsule, one row each', time: '1h' },
        { s: 'Plan', who: 'You', av: 'you', text: 'Approved the plan (2 tasks, 1 agent)', time: '1h' },
        { s: 'Implement', who: 'Worker t1 · Claude', av: 'C', text: 'Handed back: conflicts grouped per file', time: '34m', diff: 'capsule/conflicts.rs | 116 ++++++++----\n1 file changed, 96 insertions(+), 20 deletions(-)' },
        { s: 'Implement', who: 'Worker t2 · Claude', av: 'C', text: 'Handed back: capsule rows and empty list', time: '26m', diff: 'capsule/view.rs      |  67 ++++++--\ncapsule/view_test.rs |  60 ++++++\n5 files changed, 118 insertions(+), 17 deletions(-)' },
        { s: 'Checks', who: 'pocketd', av: 'sys', text: 'All 4 checks passed on the first try', time: '23m' },
        { s: 'PR review', who: 'pocketd', av: 'sys', text: 'Opened PR #203, team-91b4d2 into main', time: '19m' },
        { s: 'PR review', who: 'Reviewer · Codex', av: 'X', text: '0 critical, 2 minor, 1 nit', time: '18m' },
        { s: 'PR review', who: 'You', av: 'you', text: 'commented', time: '12m', quote: 'Looking at the re-sort finding before I merge. Order matters when the lead resolves files one at a time.' },
      ],
    },
  },
});
