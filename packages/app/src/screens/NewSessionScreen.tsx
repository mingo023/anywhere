import React, { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from "react-native";
import { CAP_LAUNCH } from "@pocket/protocol/constants";
import type { Project } from "@pocket/protocol";
import { d, font } from "../design";
import { useSession } from "../session";
import { loadPicks, savePicks } from "../launchPicks";
import {
  ACCESSES,
  ACCESS_LABEL,
  LOCKED,
  NO_PLAN,
  blocker,
  canPlan,
  clamp,
  failure,
  installed,
  locked,
  open,
  pickProvider,
  remember,
  spec,
  worktreeName,
  type Draft,
  type Picks,
  type Provider,
} from "../launch";

type Menu = "project" | "checkout" | "agent" | "access";

const PROVIDER_LABEL: Record<string, string> = { claude: "Claude", codex: "Codex" };

function taken(p: Project): string[] {
  return p.worktrees.flatMap((w) => [w.name, w.branch]).filter(Boolean);
}

function Chip({ label, on, dim, onPress }: { label: string; on?: boolean; dim?: boolean; onPress: () => void }) {
  return (
    <Pressable style={[styles.chip, on && styles.chipOn, dim && styles.dim]} onPress={onPress}>
      <Text style={styles.chipText}>{label}</Text>
    </Pressable>
  );
}

function Row({ label, hint, on, disabled, onPress }: { label: string; hint?: string; on: boolean; disabled?: boolean; onPress: () => void }) {
  return (
    <Pressable style={[styles.row, disabled && styles.dim]} disabled={disabled} onPress={onPress}>
      <View style={styles.rowText}>
        <Text style={styles.rowLabel}>{label}</Text>
        {hint ? <Text style={styles.hint}>{hint}</Text> : null}
      </View>
      {on ? <Text style={styles.check}>✓</Text> : null}
    </Pressable>
  );
}

export function NewSessionScreen({ onClose, onCreated }: { onClose: () => void; onCreated: (agentId: string) => void }) {
  const { link, caps, projects, providers, launch, openLaunch, create } = useSession();
  const [picks, setPicks] = useState<Picks>();
  const [draft, setDraft] = useState<Draft>();
  const [prompt, setPrompt] = useState("");
  const [menu, setMenu] = useState<Menu>();
  const [request, setRequest] = useState<string>();
  const [seed] = useState(() => Math.floor(Math.random() * 64));
  const supported = caps.includes(CAP_LAUNCH);

  useEffect(() => {
    if (supported) openLaunch();
    void loadPicks().then(setPicks);
  }, [supported, openLaunch]);

  useEffect(() => {
    if (!draft && picks && projects && providers) setDraft(open(picks, projects, providers.phoneMaxAccess, providers.list));
  }, [draft, picks, projects, providers]);

  const mine = launch?.requestId === request ? launch : undefined;
  const fail = mine?.failure ? failure(mine.failure.code, mine.failure.message, mine.failure.detail) : undefined;
  const refetch = fail?.refetch === true;
  const agentId = mine?.agentId;

  useEffect(() => {
    if (refetch) openLaunch();
  }, [refetch, openLaunch]);

  useEffect(() => {
    if (agentId) onCreated(agentId);
  }, [agentId, onCreated]);

  const list = providers?.list ?? [];
  const maxAccess = providers?.phoneMaxAccess ?? "ask";

  useEffect(() => {
    setDraft((d) => d && clamp(d, maxAccess));
  }, [maxAccess]);
  const project = projects?.find((p) => p.path === draft?.project);
  const worktrees = [...(project?.worktrees ?? [])].sort((a, b) => Number(b.isMain) - Number(a.isMain) || a.name.localeCompare(b.name));
  const worktree = worktrees.find((w) => w.path === draft?.worktree);
  const name = worktreeName(prompt, seed, project ? taken(project) : []);
  const block = blocker(caps, projects, providers?.list);
  const pending = request !== undefined && !mine?.failure && !mine?.agentId;
  const ready =
    link.state === "online" && !!draft && !block && prompt.trim() !== "" && !pending && !locked(draft.access, maxAccess) && installed(list, draft.provider);
  const plannable = !!draft && canPlan(list, draft.provider);

  const set = (next: Draft) => {
    setDraft(next);
    setMenu(undefined);
  };
  const toggle = (m: Menu) => setMenu(menu === m ? undefined : m);
  const send = () => {
    if (!draft || !picks || !ready) return;
    const next = remember(picks, draft);
    setPicks(next);
    savePicks(next).catch(() => {});
    setRequest(create(spec(draft, name, prompt)));
  };

  const rows = (d: Draft) => {
    switch (menu) {
      case "project":
        return (projects ?? []).map((p) => (
          <Row
            key={p.path}
            label={p.name}
            on={p.path === d.project}
            onPress={() => set(open({ ...(picks ?? { projects: {} }), last: p.path }, projects ?? [], maxAccess, list) ?? d)}
          />
        ));
      case "checkout":
        return [
          ...worktrees.map((w) => (
            <Row key={w.path} label={w.name} hint={w.branch ? `on ${w.branch}` : undefined} on={w.path === d.worktree} onPress={() => set({ ...d, worktree: w.path })} />
          )),
          <Row key="new" label="New worktree" on={d.worktree === undefined} onPress={() => set({ ...d, worktree: undefined })} />,
        ];
      case "agent":
        return list.map((p) => (
          <Row
            key={p.id}
            label={PROVIDER_LABEL[p.id] ?? p.id}
            hint={p.available ? undefined : "Not installed"}
            on={p.id === d.provider}
            disabled={!p.available}
            onPress={() => set(pickProvider(d, list, p.id as Provider))}
          />
        ));
      case "access":
        return ACCESSES.map((a) => (
          <Row
            key={a}
            label={ACCESS_LABEL[a]}
            hint={locked(a, maxAccess) ? LOCKED : undefined}
            on={a === d.access}
            disabled={locked(a, maxAccess)}
            onPress={() => set({ ...d, access: a })}
          />
        ));
      default:
        return null;
    }
  };

  return (
    <View style={styles.root}>
      <View style={styles.header}>
        <Pressable onPress={onClose}>
          <Text style={styles.cancel}>Cancel</Text>
        </Pressable>
        <Text style={styles.heading}>New session</Text>
        <View style={styles.spacer} />
      </View>
      <ScrollView contentContainerStyle={styles.body} keyboardShouldPersistTaps="handled">
        <Text style={styles.ask}>What should we work on{project ? ` in ${project.name}` : ""}?</Text>
        <View style={styles.composer}>
          <TextInput
            style={styles.input}
            multiline
            value={prompt}
            onChangeText={setPrompt}
            placeholder="Describe what the agent should do…"
            placeholderTextColor={d.faint}
          />
          <View style={styles.chips}>
            {draft ? (
              <>
                <Chip label={project?.name ?? "Project"} on={menu === "project"} onPress={() => toggle("project")} />
                <Chip label={PROVIDER_LABEL[draft.provider] ?? draft.provider} on={menu === "agent"} onPress={() => toggle("agent")} />
                <Chip label={ACCESS_LABEL[draft.access]} on={menu === "access"} onPress={() => toggle("access")} />
                <Chip label="Plan first" on={draft.plan} dim={!plannable} onPress={() => plannable && setDraft({ ...draft, plan: !draft.plan })} />
              </>
            ) : null}
            <Pressable style={[styles.send, !ready && styles.dim]} disabled={!ready} onPress={send} accessibilityLabel="Start session">
              {pending ? <ActivityIndicator color={d.bg} /> : <Text style={styles.sendText}>↑</Text>}
            </Pressable>
          </View>
        </View>
        {draft ? (
          <View style={styles.checkout}>
            <Chip label={`⎇ ${worktree?.name ?? "New worktree"}`} on={menu === "checkout"} onPress={() => toggle("checkout")} />
            {worktree ? null : <Text style={styles.branch}>branch {name}</Text>}
            {fail?.name ? <Text style={styles.error}>{fail.message}</Text> : null}
          </View>
        ) : null}
        {draft && menu ? <View style={styles.menu}>{rows(draft)}</View> : null}
        {draft && !plannable ? <Text style={styles.hint}>{NO_PLAN}</Text> : null}
        {block ? (
          <View>
            <Text style={styles.error}>{block.title}</Text>
            {block.body ? <Text style={styles.hint}>{block.body}</Text> : null}
          </View>
        ) : null}
        {fail && !fail.name ? (
          <View>
            <Text style={styles.error}>{fail.message}</Text>
            {fail.detail ? <Text style={styles.detail}>{fail.detail}</Text> : null}
          </View>
        ) : null}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
  header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingHorizontal: 16, paddingVertical: 12 },
  cancel: { color: d.muted, fontFamily: font.medium, fontSize: 15, width: 60 },
  heading: { color: d.text, fontFamily: font.semibold, fontSize: 16 },
  spacer: { width: 60 },
  body: { paddingHorizontal: 16, paddingBottom: 40, gap: 12 },
  ask: { color: d.text, fontFamily: font.semibold, fontSize: 20 },
  composer: { backgroundColor: d.card, borderColor: d.cardRule, borderWidth: 1, borderRadius: 16, padding: 12, gap: 10 },
  input: { color: d.text, fontFamily: font.regular, fontSize: 16, minHeight: 96, textAlignVertical: "top" },
  chips: { flexDirection: "row", flexWrap: "wrap", alignItems: "center", gap: 6 },
  chip: { backgroundColor: d.chip, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 6 },
  chipOn: { backgroundColor: d.stroke },
  chipText: { color: d.body, fontFamily: font.medium, fontSize: 13 },
  dim: { opacity: 0.4 },
  send: { marginLeft: "auto", width: 32, height: 32, borderRadius: 16, backgroundColor: d.green, alignItems: "center", justifyContent: "center" },
  sendText: { color: d.bg, fontFamily: font.semibold, fontSize: 16 },
  checkout: { flexDirection: "row", alignItems: "center", gap: 8 },
  branch: { color: d.muted, fontFamily: font.mono, fontSize: 12 },
  menu: { backgroundColor: d.card, borderColor: d.cardRule, borderWidth: 1, borderRadius: 12, padding: 4 },
  row: { flexDirection: "row", alignItems: "center", paddingHorizontal: 10, paddingVertical: 9, borderRadius: 8 },
  rowText: { flex: 1, gap: 2 },
  rowLabel: { color: d.text, fontFamily: font.medium, fontSize: 14 },
  hint: { color: d.faint, fontFamily: font.regular, fontSize: 12 },
  check: { color: d.green, fontSize: 14 },
  error: { color: d.red, fontFamily: font.medium, fontSize: 13 },
  detail: { color: d.muted, fontFamily: font.mono, fontSize: 12, marginTop: 4 },
});
