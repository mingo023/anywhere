import type { AgentSummary } from "@pocket/protocol";

export type Tone = "warn" | "error" | "accent" | "ok" | "muted";

export const RANK = { needsYou: 0, failed: 1, done: 2, working: 3, idle: 4 } as const;

type Look = { rank: number; tone: Tone; label?: string };

const idle: Look = { rank: RANK.idle, tone: "muted" };

export function look(agent: AgentSummary): Look {
  if (!agent.attached) return idle;
  switch (agent.status) {
    case "needsYou":
      return { rank: RANK.needsYou, tone: "warn", label: "Needs you" };
    case "done":
      return agent.failed
        ? { rank: RANK.failed, tone: "error", label: "Failed" }
        : { rank: RANK.done, tone: "ok", label: "Done" };
    case "working":
      return { rank: RANK.working, tone: "accent", label: "Working" };
    default:
      return idle;
  }
}

const notices = new Map([
  ["resumed", "Resumed"],
  ["interrupted", "Interrupted by restart"],
  ["access_lowered", "Full access resumed as Ask"],
  ["failed", "Couldn't resume"],
]);

/** How a pocketd restart brought the agent back, until its next turn. */
export function restoreNotice(agent: AgentSummary): string | undefined {
  return notices.get(agent.restore ?? "");
}
