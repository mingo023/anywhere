import type { ToolCall } from "@pocket/protocol";

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

export function toolSummary(calls: readonly ToolCall[]): string {
  const kinds = (kind: ToolCall["detail"]["kind"]) => calls.filter((c) => c.detail.kind === kind).length;
  const edited = new Set(
    calls.flatMap(({ detail }) => (detail.kind === "edit" || detail.kind === "write" ? [detail.path] : [])),
  ).size;
  const failed = calls.filter((c) => c.status === "error").length;
  const parts = [
    kinds("shell") && `ran ${count(kinds("shell"), "command", "commands")}`,
    edited && `edited ${count(edited, "file", "files")}`,
    kinds("read") && `read ${count(kinds("read"), "file", "files")}`,
    kinds("search") && `searched ${count(kinds("search"), "time", "times")}`,
    kinds("task") && `started ${count(kinds("task"), "task", "tasks")}`,
    kinds("other") && `called ${count(kinds("other"), "tool", "tools")}`,
    failed && `${failed} failed`,
  ].filter((part): part is string => !!part);
  const text = parts.join(" · ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export function groupOpen(calls: readonly ToolCall[], live: boolean, override?: boolean): boolean {
  return override ?? (live || calls.some((c) => c.status === "running"));
}
