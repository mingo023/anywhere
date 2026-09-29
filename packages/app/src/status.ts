import type { AgentSummary } from "@pocket/protocol";

export type Tone = "warn" | "error" | "accent" | "ok" | "muted";

type Look = { rank: number; tone: Tone; label?: string };

const idle: Look = { rank: 4, tone: "muted" };

export function look(agent: AgentSummary): Look {
  if (!agent.attached) return idle;
  switch (agent.status) {
    case "needsYou":
      return { rank: 0, tone: "warn", label: "Needs you" };
    case "done":
      return agent.failed ? { rank: 1, tone: "error", label: "Failed" } : { rank: 2, tone: "accent", label: "Done" };
    case "working":
      return { rank: 3, tone: "ok", label: "Working" };
    default:
      return idle;
  }
}

export function byUrgency(a: AgentSummary, b: AgentSummary): number {
  return look(a).rank - look(b).rank || b.updatedAt - a.updatedAt;
}
