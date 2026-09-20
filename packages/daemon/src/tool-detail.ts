import type { ToolDetail } from "@pocket/protocol";
import { fileDiff } from "./diff.js";

function str(input: Record<string, unknown>, key: string): string | undefined {
  const value = input[key];
  return typeof value === "string" ? value : undefined;
}

export function toolDetail(name: string, rawInput: unknown): ToolDetail {
  const input = (typeof rawInput === "object" && rawInput ? rawInput : {}) as Record<string, unknown>;

  switch (name) {
    case "Bash":
    case "BashOutput": {
      const command = str(input, "command") ?? "";
      const description = str(input, "description");
      return description ? { kind: "shell", command, description } : { kind: "shell", command };
    }
    case "Read":
    case "NotebookRead":
      return { kind: "read", path: str(input, "file_path") ?? "" };
    case "Edit":
    case "NotebookEdit": {
      const path = str(input, "file_path") ?? "";
      const newString = str(input, "new_string") ?? str(input, "new_source");
      if (newString === undefined) return { kind: "edit", path };
      return { kind: "edit", path, diff: fileDiff(str(input, "old_string") ?? str(input, "old_source"), newString) };
    }
    case "Write": {
      const path = str(input, "file_path") ?? "";
      const content = str(input, "content");
      return content === undefined ? { kind: "write", path } : { kind: "write", path, diff: fileDiff(undefined, content) };
    }
    case "Grep":
    case "Glob": {
      const query = str(input, "pattern") ?? "";
      const path = str(input, "path");
      return path ? { kind: "search", query, path } : { kind: "search", query };
    }
    case "Task":
    case "Agent":
      return { kind: "task", description: str(input, "description") ?? "" };
    default:
      return { kind: "other", name, input: rawInput };
  }
}
