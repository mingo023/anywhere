import type { AgentSummary } from "@pocket/protocol";

export type ContextTone = "muted" | "warning" | "danger";
export type ContextChip = { percent: number; tone: ContextTone };

/** The composer's "NN% context" chip, or undefined until pocketd reports the window. */
export const contextChip = (a: AgentSummary): ContextChip | undefined => {
  if (!a.contextWindow) return undefined;
  const percent = Math.min(100, Math.floor(((a.tokensUsed ?? 0) * 100) / a.contextWindow));
  return { percent, tone: percent >= 90 ? "danger" : percent >= 75 ? "warning" : "muted" };
};
