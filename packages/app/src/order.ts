import type { AgentSummary } from "@pocket/protocol";
import { look, RANK } from "./status.ts";

export function allSessions(agents: readonly AgentSummary[]): AgentSummary[] {
  return [...agents].sort((a, b) => b.createdAt - a.createdAt || a.id.localeCompare(b.id));
}

/** Ties go to whatever has sat longest in its status. Only a status step moves `updatedAt`, so ascending means oldest transition first. */
export function upNext(agents: readonly AgentSummary[]): AgentSummary[] {
  return agents
    .filter((a) => look(a).rank <= RANK.done)
    .sort((a, b) => look(a).rank - look(b).rank || a.updatedAt - b.updatedAt || a.id.localeCompare(b.id));
}

function needYou(count: number): string {
  return `${count} ${count === 1 ? "needs" : "need"} you`;
}

export function summary(agents: readonly AgentSummary[]): string | null {
  if (!agents.length) return null;
  const count = (rank: number) => agents.filter((a) => look(a).rank === rank).length;
  const needs = count(RANK.needsYou);
  const failed = count(RANK.failed);
  const done = count(RANK.done);
  const working = count(RANK.working);
  const parts = [
    needs ? needYou(needs) : "",
    failed ? `${failed} failed` : "",
    done ? `${done} done` : "",
    working ? `${working} working` : "",
  ].filter(Boolean);
  if (parts.length) return parts.join(" · ");
  return agents.length === 1 ? "1 session" : `${agents.length} sessions`;
}

export function needsYouElsewhere(agents: readonly AgentSummary[], agentId: string): number {
  return agents.filter((a) => a.id !== agentId && look(a).rank === RANK.needsYou).length;
}

export function backLabel(needsYou: number): string {
  return needsYou ? `Back to sessions, ${needYou(needsYou)}` : "Back to sessions";
}
