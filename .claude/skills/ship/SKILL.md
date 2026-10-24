---
name: ship
description: Commit the current changes on a branch and open a PR to main with the right `type:` label, without asking questions. Only runs when invoked as /ship.
disable-model-invocation: true
---

# Ship

Invoking /ship is the approval: commit, push and open the PR without asking. Decide everything below from the diff and the conversation. Stop only if there is nothing to commit, or a git/gh command fails in a way you can't fix.

## Steps

1. **Look:** `git status`, `git diff HEAD`, `git log -5 --oneline`. Skip files that look accidental (secrets, `.env`, build output, stray scratch files) and say so at the end.
2. **Branch:** on `main`, create a short kebab-case branch named for the change (`fix-table-chip-clip`, `chat-prompt-glide`). Already on a feature branch: stay on it.
3. **Commit:** stage the files explicitly (not `git add -A`) and commit with a HEREDOC message.
4. **Push:** `git push -u origin HEAD`.
5. **PR:** `gh pr create --base main --title "<commit title>" --label "<type label>" --body "<body>"`. If the branch already has an open PR, push only and update its label if it's wrong.
6. **Report:** the PR URL, the label, and anything skipped.

## Commit message

Match the history: the title is a plain sentence saying what the app now does, from the user's side, no prefix, no trailing period, under ~72 chars.

- `Chat code blocks select and scroll sideways`
- `Sidebar rows pin only from their menu`
- `Markdown table columns stop clipping a file chip`

The body (wrapped at 72) says what was wrong or missing and what changed, in a few sentences. End with:

```
Co-authored-by: Claude <noreply@anthropic.com>
```

## Label

Every PR needs exactly one `type:` label; CI (`pr-type-label.yml`) fails otherwise, and it decides the CHANGELOG heading (`.github/release.yml`).

| Label | When |
|---|---|
| `type: feature` | New or changed behaviour a user would notice |
| `type: fix` | Something was broken and now works |
| `type: docs` | Only docs change (`docs/`, README, ADRs) |
| `type: internal` | CI, tooling, refactors, tests, skills, `.claude/`: nothing a user sees; left out of the changelog |

Pick by what the user experiences, not by file type. A refactor that also fixes a visible bug is `type: fix`.

## PR body

```markdown
## TLDR
<one or two sentences>

## Why
- <the problem or motivation>

## What does this PR do
- `<file or module>`: <change>

## Not verified
- <what wasn't built, tested or captured; omit the section if everything was>
```

Keep it short and factual; name real files and functions.
