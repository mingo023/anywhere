# Wayfinder (local markdown tracker)

Each map is a folder `docs/wayfinder/<map>/`.

- `map.md`: the map issue, label `wayfinder:map`.
- `NN-<slug>.md`: child tickets; `NN` is the id. Frontmatter:

```yaml
id: 3
title: Ticket name
labels: [wayfinder:research]   # research | prototype | grilling | task
status: open                   # open | closed
assignee:                      # claimant; empty = unclaimed
blocked_by: [1, 2]             # ids of blocking tickets
```

## Wayfinding operations

- **Claim:** set `assignee` before any work.
- **Unblocked:** every id in `blocked_by` has `status: closed`.
- **Frontier:** `open`, unblocked, empty `assignee`.
- **Resolve:** append `## Resolution`, set `status: closed`, add one line to the map's `## Decisions so far`.
- **Assets:** link files or branches; don't paste them in.
