import React, { useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from "react-native";
import type { FileDiff, ToolCall, ToolDetail } from "@pocket/protocol";
import { d, font } from "../design";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Cross,
  File,
  type IconProps,
  Pencil,
  Search,
  Target,
  Terminal,
} from "../icons";
import { groupOpen, toolSummary } from "../tools";
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

function Row({ call }: { call: ToolCall }) {
  const [open, setOpen] = useState(false);
  const { Glyph, name, text } = describe(call.detail);
  const diff = diffOf(call.detail);
  const failed = call.status === "error";
  const expandable = !!diff || !!call.output;

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

      {open && call.output ? <Text style={[styles.output, failed && { color: d.red }]}>{call.output}</Text> : null}
    </View>
  );
}

export function ToolGroup({ calls, live }: { calls: readonly ToolCall[]; live: boolean }) {
  const [override, setOverride] = useState<boolean>();
  const open = groupOpen(calls, live, override);

  return (
    <View style={styles.card}>
      <Pressable style={styles.header} onPress={() => setOverride(!open)}>
        <Text style={styles.headerText}>{toolSummary(calls)}</Text>
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
