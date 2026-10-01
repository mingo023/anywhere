import type { FileDiff, TimelineItem } from "@pocket/protocol";

export type ChangedFile = { path: string; edits: readonly FileDiff[]; additions: number; deletions: number };

export type ChangeRow =
  | { kind: "file"; key: string; file: ChangedFile; open: boolean }
  | { kind: "edit"; key: string; path: string; index: number; count: number; diff: FileDiff };

/** Each file's +/- is its latest edit, so re-touching a file does not count its lines twice. */
export function changedFiles(items: readonly TimelineItem[]): ChangedFile[] {
  const edits = new Map<string, FileDiff[]>();
  for (const item of items) {
    if (item.kind !== "tool") continue;
    const { detail } = item.call;
    if ((detail.kind !== "edit" && detail.kind !== "write") || !detail.diff) continue;
    const list = edits.get(detail.path);
    if (list) list.push(detail.diff);
    else edits.set(detail.path, [detail.diff]);
  }
  return [...edits].map(([path, list]) => {
    const latest = list[list.length - 1]!;
    return { path, edits: list, additions: latest.additions, deletions: latest.deletions };
  });
}

export function totals(files: readonly ChangedFile[]): { added: number; removed: number } {
  return files.reduce(
    (sum, file) => ({ added: sum.added + file.additions, removed: sum.removed + file.deletions }),
    { added: 0, removed: 0 },
  );
}

export function changeRows(files: readonly ChangedFile[], open: ReadonlySet<string>): ChangeRow[] {
  return files.flatMap((file): ChangeRow[] => {
    const head: ChangeRow = { kind: "file", key: `f:${file.path}`, file, open: open.has(file.path) };
    if (!open.has(file.path)) return [head];
    const count = file.edits.length;
    return [
      head,
      ...file.edits.map((diff, index): ChangeRow => ({
        kind: "edit",
        key: `e:${file.path}:${index}`,
        path: file.path,
        index,
        count,
        diff,
      })),
    ];
  });
}
