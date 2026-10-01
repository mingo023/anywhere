# Orchestrator research: Zeron + MonoCode → Coding Pocket

Research, product decisions and implementable plans for bringing Zeron's and MonoCode's ideas and UI/UX into Pocket.

**Tóm tắt:** Nghiên cứu Zeron, MonoCode và các orchestrator khác (research 01–19), chọn chiến lược **C spine + A cockpit**: pocketd điều khiển cả fleet, desktop là buồng lái terminal (04). PRD, UX spec và roadmap 8 tuần nằm ở product 00–06. Mỗi epic plan_now có một design doc và một plan đã review, chạy thử PR độc lập đầu tiên trong worktree nếu có (07).

## Reading order

1. `product/04-strategy-decision.md`: what was chosen and why
2. `product/00-prd.md`: scope, decisions D1–D43, owner questions
3. `product/05-roadmap.md`: epics, PR-level dependencies, rules (§7) and spec overrides (§7.1)
4. `product/07-plan-index.md`: the plans, their execution order and file collisions
5. Specs, feature matrix and research as needed

## Product

| File | Contents |
|---|---|
| `product/00-prd.md` | PRD for the next 8 weeks, decisions log D1–D43 |
| `product/01-feature-matrix.md` | Zeron, MonoCode and landscape features vs Pocket (362 ideas) |
| `product/02-ux-spec-desktop.md` | Desktop UX spec: tokens, layout, keymap, copy |
| `product/03-ux-spec-phone.md` | Phone UX spec |
| `product/04-strategy-decision.md` | Judge scores A 70 / B 58 / C 81, grafts, rejections |
| `product/05-roadmap.md` | Milestones M0–M3, epics E01–E17 |
| `product/06-prd-review.md` | Red-team findings and how each was resolved |
| `product/07-plan-index.md` | Plan table, two-lane PR order, collisions |
| `product/strategy/A-terminal-cockpit.md` | Candidate A |
| `product/strategy/B-dual-mode.md` | Candidate B |
| `product/strategy/C-control-plane.md` | Candidate C (winner, with A's cockpit grafted) |

## Research

| File | Topic |
|---|---|
| `research/01-zeron-product.md` | Zeron product and features |
| `research/02-zeron-architecture.md` | Zeron engine, sync, transport |
| `research/03-zeron-harness.md` | How Zeron drives each agent |
| `research/04-zeron-desktop-shell-ux.md` | Zeron sidebar, palette, chrome |
| `research/05-zeron-transcript-composer-ux.md` | Zeron transcript, composer, notifications |
| `research/06-zeron-git-files-ux.md` | Zeron git, diff, files, terminal |
| `research/07-zeron-design-system.md` | Zeron tokens mapped to Pocket |
| `research/08-zeron-mobile.md` | Zeron iOS app and push |
| `research/09-monocode-product.md` | MonoCode product and features |
| `research/10-monocode-architecture-harness.md` | MonoCode architecture and provider harness |
| `research/11-monocode-orchestration.md` | MonoCode orchestration, automations, CLI, inbox |
| `research/12-monocode-shell-design.md` | MonoCode shell and tokens |
| `research/13-monocode-session-git-ux.md` | MonoCode session, composer, source control |
| `research/14-pocket-baseline.md` | Pocket today |
| `research/15-landscape.md` | Other orchestrators |
| `research/16-terminal-surface.md` | Terminal selection, scrollback, tabs |
| `research/17-new-session-flow.md` | New-session flow and agent setup |
| `research/18-trust-model.md` | Pairing, device tokens, reach, agent control |
| `research/19-install-onboarding.md` | Install, first run, distribution |

## Designs and plans

One design (`docs/designs/2026-09-30-<slug>.md`) and one plan (`docs/plans/2026-09-30-<slug>.md`) per plan_now epic. Each plan was reviewed statically, had its first independent PR dry-run in a throwaway worktree where one exists, and ends with a `## Verification` section. `product/07-plan-index.md` has the two-lane execution order, file collisions, cross-plan fixes, owner questions and PO-decided items.

| Epic | Slug | PRs | Dry run |
|---|---|---|---|
| E01 | `fix-now` | 5 | PR1: automatable steps pass; notification banner is manual |
| E02 | `reach-lockdown` | 5 | PR1 pass |
| E03 | `no-self-approval` | 5 | none (needs E02) |
| E04 | `always-on` | 4 | PR4 pass |
| E05 | `registry-worktrees` | 3 | partial (needs E02, E03) |
| E06 | `launchspec-create` | 5 | none (needs E02–E05) |
| E07 | `terminal-surface` | 5 | PR4 pass |
| E08 | `desktop-attention` | 5 | none (needs E16 PR1) |
| E16 | `look` | 5 | PR1 pass |
| E17 | `phone-shell` | 5 | PR1 pass |
| E09 | `restore` | 5 | PR1 pass |

Decisions an agent made without asking are marked **PO-decided — review**.

## Caveats

- Plans target main `5091a01`. Research citations to `packages/desktop/crates/pocket/src/…` predate the pocket crate split (ADR 0003); `product/05-roadmap.md` §0 maps old paths to new.
- Designs were written at `f8f7293` and predate three owner commits: the dark theme (merged at `1e71e24`), dropping a session when its agent exits (`8a10124`), and terminal mouse selection with ⌘C (`5091a01`). Plans build on the merged code; where that changed a design decision, the design's log says "Rebased on 5091a01".
- The dark theme (`docs/plans/2026-09-30-dark-theme.md`) wins over UXD §2 and E16.
