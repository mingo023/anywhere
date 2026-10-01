import { CAP_LAUNCH } from "@pocket/protocol/constants";
import { MALFORMED } from "./connection.ts";
import type { LaunchSpec, PhoneAccess, Project, ProviderInfo } from "@pocket/protocol";

export type Provider = LaunchSpec["provider"];
export type LaunchPick = { provider: Provider; model?: string; effort?: string; access: PhoneAccess };
/** The last Project, and each Project's agent and access. Plan first is never kept. */
export type Picks = { last?: string; projects: Record<string, LaunchPick> };
/** The sheet's chips. `worktree` is a Worktree path, or undefined for a new Worktree. */
export type Draft = { project: string; worktree?: string; provider: Provider; model?: string; effort?: string; access: PhoneAccess; plan: boolean };
export type Failure = { message: string; detail?: string; name?: boolean; refetch?: boolean };
/** The phone's latest create. `id` is its message id, which pocketd's `error` echoes; `sentAt` is when it was first sent. */
export type Launch = { requestId: string; id: string; sentAt: number; spec: LaunchSpec; agentId?: string; failure?: { code?: string; message: string; detail?: string } };

export const ACCESSES: readonly PhoneAccess[] = ["ask", "edits", "auto"];
export const ACCESS_LABEL: Record<PhoneAccess, string> = { ask: "Ask", edits: "Auto-accept edits", auto: "Auto" };
export const LOCKED = "On your Mac: ⌘K → Phone access level";
export const NO_PLAN = "Codex can't plan first in a terminal session";
export const UNCONFIRMED = "Lost touch with your Mac. Check its sessions before trying again.";
/** How long pocketd keeps a create's reply for a resend to replay. */
export const RECEIPT_TTL_MS = 10 * 60_000;

const ADJECTIVES = ["brave", "calm", "eager", "fuzzy", "keen", "lucky", "quiet", "swift"];
const NOUNS = ["otter", "heron", "maple", "comet", "falcon", "cedar", "koala", "lynx"];

export function spec(d: Draft, name: string, prompt: string): LaunchSpec {
  return {
    project: d.project,
    checkout: d.worktree === undefined ? { new: { name } } : { worktree: d.worktree },
    provider: d.provider,
    ...(d.model === undefined ? {} : { model: d.model }),
    ...(d.effort === undefined ? {} : { effort: d.effort }),
    access: d.access,
    plan: d.plan,
    prompt: prompt.trim(),
  };
}

/** An unknown ceiling counts as "ask", pocketd's default. */
function ceiling(maxAccess: string): number {
  return Math.max(0, ACCESSES.indexOf(maxAccess as PhoneAccess));
}

export function locked(access: PhoneAccess, maxAccess: string): boolean {
  return ACCESSES.indexOf(access) > ceiling(maxAccess);
}

export function clamp(d: Draft, maxAccess: string): Draft {
  return locked(d.access, maxAccess) ? { ...d, access: ACCESSES[ceiling(maxAccess)]! } : d;
}

export function canPlan(providers: readonly ProviderInfo[], provider: Provider): boolean {
  return providers.some((p) => p.id === provider && p.plan);
}

export function installed(providers: readonly ProviderInfo[], provider: Provider): boolean {
  return providers.some((p) => p.id === provider && p.available);
}

export function pickProvider(d: Draft, providers: readonly ProviderInfo[], provider: Provider): Draft {
  const next = { ...d, provider, plan: d.plan && canPlan(providers, provider) };
  if (provider !== d.provider) {
    delete next.model;
    delete next.effort;
  }
  return next;
}

/** A fresh draft in the last Project, else the first, with that Project's picks. A remembered agent no longer installed gives way to the first installed one. */
export function open(picks: Picks, projects: readonly Project[], maxAccess: string, providers: readonly ProviderInfo[]): Draft | undefined {
  const project = projects.find((p) => p.path === picks.last) ?? projects[0];
  if (!project) return undefined;
  const pick = picks.projects[project.path];
  const access = pick && !locked(pick.access, maxAccess) ? pick.access : "ask";
  const remembered = pick?.provider ?? "claude";
  const fallback = providers.find((p) => p.available)?.id as Provider | undefined;
  const provider = installed(providers, remembered) ? remembered : (fallback ?? remembered);
  const draft: Draft = { project: project.path, worktree: project.worktrees[0]?.path, provider, access, plan: false };
  if (pick && provider === pick.provider) {
    if (pick.model !== undefined) draft.model = pick.model;
    if (pick.effort !== undefined) draft.effort = pick.effort;
  }
  return draft;
}

export function remember(picks: Picks, d: Draft): Picks {
  const pick: LaunchPick = { provider: d.provider, access: d.access };
  if (d.model !== undefined) pick.model = d.model;
  if (d.effort !== undefined) pick.effort = d.effort;
  return { last: d.project, projects: { ...picks.projects, [d.project]: pick } };
}

export function slug(prompt: string): string {
  return prompt.split(/[^A-Za-z0-9]+/).filter(Boolean).slice(0, 4).join("-").toLowerCase();
}

export function worktreeName(prompt: string, seed: number, taken: readonly string[]): string {
  const used = new Set(taken.map((t) => (t.split("/")[0] ?? t).toLowerCase()));
  const free = (name: string) => !used.has(name.toLowerCase());
  const unique = (base: string) => {
    let name = base;
    for (let n = 2; !free(name); n++) name = `${base}-${n}`;
    return name;
  };
  const s = slug(prompt);
  if (s) return unique(s);
  const count = ADJECTIVES.length * NOUNS.length;
  for (let i = 0; i < count; i++) {
    const n = (seed + i) % count;
    const name = `${ADJECTIVES[Math.floor(n / NOUNS.length)]}-${NOUNS[n % NOUNS.length]}`;
    if (free(name)) return name;
  }
  return unique("worktree");
}

/**
 * A create still pending after a reconnect is sent again under its requestId, so pocketd joins the first one or replays its reply.
 * Once pocketd may have dropped that reply, a resend could start a second Session, so the create fails instead.
 */
export function reconnect(launch: Launch | undefined, now: number): { launch: Launch; send?: { type: "agent.create"; requestId: string; spec: LaunchSpec } } | undefined {
  if (!launch || launch.agentId || launch.failure) return undefined;
  if (now - launch.sentAt >= RECEIPT_TTL_MS) return { launch: { ...launch, failure: { message: UNCONFIRMED } } };
  return { launch, send: { type: "agent.create", requestId: launch.requestId, spec: launch.spec } };
}

/** `name` failures show under the new Worktree's name. */
export function failure(code: string | undefined, message: string, detail?: string): Failure {
  switch (code) {
    case "invalid_name":
      return { message: "Use letters, digits, - _ or .", name: true };
    case "worktree_exists":
      return { message: "A worktree or branch with this name already exists", name: true };
    case "folder_not_trusted":
      return { message: "Trust this folder in Claude on your Mac first" };
    case "access_not_allowed":
      return { message: LOCKED, refetch: true };
    default:
      if (!code && message === MALFORMED) return { message: "Update Pocket on your Mac." };
      return detail ? { message, detail } : { message };
  }
}

export function blocker(
  caps: readonly string[],
  projects: readonly Project[] | undefined,
  providers: readonly ProviderInfo[] | undefined,
): { title: string; body?: string } | undefined {
  if (!caps.includes(CAP_LAUNCH)) return { title: "Update Pocket on your Mac." };
  if (projects?.length === 0) return { title: "Add a project on your Mac first." };
  if (providers && !providers.some((p) => p.available)) {
    return { title: "No agents available", body: "Install Claude Code or Codex, then reopen." };
  }
  return undefined;
}
