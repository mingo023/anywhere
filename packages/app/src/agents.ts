import type { AgentSummary } from "@pocket/protocol";

export function applyAgentUpdate(agents: readonly AgentSummary[], agent: AgentSummary): readonly AgentSummary[] {
  if (agent.status === "closed") return agents.filter((a) => a.id !== agent.id);
  const at = agents.findIndex((a) => a.id === agent.id);
  if (at < 0) return [agent, ...agents];
  const next = agents.slice();
  next[at] = agent;
  return next;
}

export function liveAgents(agents: readonly AgentSummary[]): readonly AgentSummary[] {
  return agents.filter((a) => a.status !== "closed");
}
