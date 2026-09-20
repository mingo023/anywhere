import React, { useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from "react-native";
import type { AgentStep, FileDiff, ToolCall, ToolDetail } from "@pocket/protocol";
import { d, font } from "../design";
import {
  Bot,
  Check,
  ChevronDown,
  ChevronRight,
  Cross,
  File,
  type IconProps,
  Minus,
  Pencil,
  Search,
  Target,
  Terminal,
} from "../icons";
import { DiffStat, DiffView } from "./DiffView";

type Glyph = (props: IconProps) => React.JSX.Element;

function describe(detail: ToolDetail): { Glyph: Glyph; name: string; text: string } {
  switch (detail.kind) {
    case "shell":
      return { Glyph: Terminal, name: "Shell", text: detail.command };
    case "edit":
      return { Glyph: Pencil, name: "Patch", text: detail.path };
    case "write":
      return { Glyph: Pencil, name: "Write", text: detail.path };
    case "read":
      return { Glyph: File, name: "Read", text: detail.path };
    case "search":
      return { Glyph: Search, name: "Search", text: detail.path ? `${detail.query} · ${detail.path}` : detail.query };
    case "task":
      return { Glyph: Target, name: "Task", text: detail.description };
    case "other":
      return { Glyph: Target, name: detail.name, text: "" };
  }
}

function diffOf(detail: ToolDetail): FileDiff | undefined {
  return detail.kind === "edit" || detail.kind === "write" ? detail.diff : undefined;
}

function Status({ status }: { status: ToolCall["status"] }) {
  if (status === "running") return <ActivityIndicator size="small" color={d.faint} />;
  return status === "ok" ? <Check size={14} color={d.green} /> : <Cross size={14} color={d.red} />;
}

function Step({ step }: { step: AgentStep }) {
  if (step.kind !== "tool") {
    const Glyph = step.kind === "reasoning" ? Minus : Bot;
    return (
      <View style={styles.stepRow}>
        <Glyph size={13} color={d.faint} />
        <Text style={styles.stepText} numberOfLines={2}>
          {step.text.trim()}
        </Text>
      </View>
    );
  }

  const { Glyph, name, text } = step.detail ? describe(step.detail) : { Glyph: Target, name: step.text, text: "" };
  return (
    <View style={styles.stepRow}>
      <Glyph size={13} color={d.faint} />
      <Text style={styles.stepName}>{name}</Text>
      <Text style={styles.stepText} numberOfLines={1}>
        {text}
      </Text>
      {step.status && step.status !== "running" ? <Status status={step.status} /> : null}
    </View>
  );
}

function Row({ call }: { call: ToolCall }) {
  const [open, setOpen] = useState(false);
  const { Glyph, name, text } = describe(call.detail);
  const diff = diffOf(call.detail);
  const run = call.agentRun;
  const failed = call.status === "error";
  const expandable = !!diff || !!run?.steps.length || !!call.output;

  return (
    <View style={styles.row}>
      <Pressable style={styles.rowHead} onPress={() => expandable && setOpen((v) => !v)}>
        <Glyph size={15} color={d.faint} />
        <Text style={[styles.rowName, failed && { color: d.red }]}>{name}</Text>
        <Text style={[styles.rowText, failed && { color: d.red }]} numberOfLines={1}>
          {text}
        </Text>
        {diff ? <DiffStat diff={diff} /> : null}
        <Status status={call.status} />
      </Pressable>

      {open && diff ? <DiffView diff={diff} /> : null}

      {open && run?.steps.length ? (
        <View style={styles.run}>
          <Text style={styles.runName}>{run.name}</Text>
          {run.steps.map((step) => (
            <Step key={step.id} step={step} />
          ))}
        </View>
      ) : null}

      {open && call.output ? <Text style={[styles.output, failed && { color: d.red }]}>{call.output}</Text> : null}
    </View>
  );
}

export function ToolGroup({ calls }: { calls: readonly ToolCall[] }) {
  const [open, setOpen] = useState(true);
  const total = calls.reduce((sum, call) => sum + (call.durationMs ?? 0), 0);
  const label = `${calls.length} action${calls.length === 1 ? "" : "s"}${total ? ` · ${Math.round(total / 1000)}s` : ""}`;

  return (
    <View style={styles.card}>
      <Pressable style={styles.header} onPress={() => setOpen((v) => !v)}>
        <Text style={styles.headerText}>{label}</Text>
        {open ? <ChevronDown size={16} color={d.muted} /> : <ChevronRight size={16} color={d.muted} />}
      </Pressable>
      {open ? calls.map((call) => <Row key={call.toolUseId} call={call} />) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: d.card,
    borderWidth: 1,
    borderColor: d.cardRule,
    borderRadius: 14,
    overflow: "hidden",
  },
  header: {
    height: 40,
    paddingHorizontal: 12,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
  },
  headerText: { color: d.muted, fontSize: 13, fontFamily: font.medium },
  row: { borderTopWidth: 1, borderTopColor: d.cardRule },
  rowHead: { height: 36, paddingHorizontal: 12, flexDirection: "row", alignItems: "center", gap: 10 },
  rowName: { color: d.text, fontSize: 12.5, fontFamily: font.monoSemibold },
  rowText: { flex: 1, color: d.muted, fontSize: 12.5, fontFamily: font.mono },
  run: { paddingLeft: 12, paddingRight: 12, paddingBottom: 8, gap: 2 },
  runName: { color: d.teal, fontSize: 11.5, fontFamily: font.monoSemibold, paddingVertical: 4 },
  stepRow: { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 2, paddingLeft: 4 },
  stepName: { color: d.body, fontSize: 11.5, fontFamily: font.monoSemibold },
  stepText: { flex: 1, color: d.faint, fontSize: 11.5, fontFamily: font.mono },
  output: {
    color: d.muted,
    fontSize: 12,
    lineHeight: 17,
    fontFamily: font.mono,
    paddingHorizontal: 12,
    paddingTop: 8,
    paddingBottom: 12,
  },
});
