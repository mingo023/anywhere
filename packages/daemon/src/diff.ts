import { DIFF_LINE_CHARS, DIFF_PREVIEW_LINES, type DiffLine, type FileDiff } from "@pocket/protocol";

const SYNC_WINDOW = 8;

function textLines(text: string): string[] {
  if (!text) return [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

function cap(text: string): string {
  return text.length <= DIFF_LINE_CHARS ? text : `${text.slice(0, DIFF_LINE_CHARS)}…`;
}

/** The nearest pair of matching lines within the window, so a small edit stays a small hunk. */
function findSync(oldLines: string[], newLines: string[], i: number, j: number): { i: number; j: number } | null {
  for (let di = 0; di <= SYNC_WINDOW; di += 1) {
    for (let dj = 0; dj <= SYNC_WINDOW; dj += 1) {
      if (di === 0 && dj === 0) continue;
      const oi = i + di;
      const nj = j + dj;
      if (oi < oldLines.length && nj < newLines.length && oldLines[oi] === newLines[nj]) return { i: oi, j: nj };
    }
  }
  return null;
}

function greedyDiff(oldLines: string[], newLines: string[]): DiffLine[] {
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;

  while (i < oldLines.length || j < newLines.length) {
    if (i < oldLines.length && j < newLines.length && oldLines[i] === newLines[j]) {
      out.push({ number: j + 1, kind: "context", text: oldLines[i]! });
      i += 1;
      j += 1;
      continue;
    }

    const sync = findSync(oldLines, newLines, i, j);
    if (sync) {
      while (i < sync.i) out.push({ number: i + 1, kind: "del", text: oldLines[i++]! });
      while (j < sync.j) out.push({ number: j + 1, kind: "add", text: newLines[j++]! });
      continue;
    }

    if (i < oldLines.length) out.push({ number: i + 1, kind: "del", text: oldLines[i++]! });
    else out.push({ number: j + 1, kind: "add", text: newLines[j++]! });
  }

  return out;
}

export function fileDiff(oldText: string | undefined, newText: string): FileDiff {
  const contentOnly = oldText === undefined || oldText === "";
  const hunks = contentOnly
    ? textLines(newText).map((text, index): DiffLine => ({ number: index + 1, kind: "add", text }))
    : greedyDiff(textLines(oldText), textLines(newText));

  const first = hunks.findIndex((line) => line.kind !== "context");
  const start = Math.max(0, first === -1 ? 0 : first - 1);

  return {
    lines: hunks.slice(start, start + DIFF_PREVIEW_LINES).map((line) => ({ ...line, text: cap(line.text) })),
    additions: hunks.filter((line) => line.kind === "add").length,
    deletions: hunks.filter((line) => line.kind === "del").length,
    ...(contentOnly ? { contentOnly: true } : {}),
  };
}
