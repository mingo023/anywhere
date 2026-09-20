import React, { useEffect, useRef, useState } from "react";
import { Animated, Easing, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import type { SessionTask } from "@pocket/protocol";
import { d, font } from "../design";
import { Asterisk, Bot, ChevronLeft, Stop, Terminal } from "../icons";
import { Glass } from "../components/Glass";
import { elapsed, useTick } from "../components/TimelineView";

type Kind = SessionTask["kind"];

const SECTIONS: { kind: Kind; label: string; icon: typeof Bot }[] = [
  { kind: "agent", label: "Sub-agents", icon: Bot },
  { kind: "shell", label: "Shells", icon: Terminal },
  { kind: "workflow", label: "Workflows", icon: Asterisk },
];

const FILTERS: { key: Kind | "all"; label: string }[] = [
  { key: "all", label: "All" },
  { key: "agent", label: "Agents" },
  { key: "shell", label: "Shells" },
  { key: "workflow", label: "Workflows" },
];

function count(value: number, unit: string): string {
  return `${value} ${unit}${value === 1 ? "" : "s"}`;
}

function tokens(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(1)}k` : String(value);
}

function Spinner() {
  const spin = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    const loop = Animated.loop(
      Animated.timing(spin, { toValue: 1, duration: 1200, easing: Easing.linear, useNativeDriver: true }),
    );
    loop.start();
    return () => loop.stop();
  }, [spin]);

  return (
    <Animated.View
      style={{ transform: [{ rotate: spin.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "360deg"] }) }] }}
    >
      <Asterisk size={14} color={d.green} />
    </Animated.View>
  );
}

function Row({ task, onStop }: { task: SessionTask; onStop: () => void }) {
  const ms = useTick(task.startedAt);

  const meta = [
    elapsed(ms),
    task.tools ? count(task.tools, "tool") : null,
    task.tokens ? `${tokens(task.tokens)} tokens` : null,
    task.detail,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <View style={styles.row}>
      <View style={styles.status}>
        <Spinner />
      </View>
      <View style={styles.rowBody}>
        <Text style={styles.rowTitle} numberOfLines={1}>
          {task.title}
        </Text>
        <Text style={styles.rowMeta} numberOfLines={1}>
          {meta}
        </Text>
      </View>
      <Pressable style={styles.stop} accessibilityLabel={`Stop ${task.title}`} onPress={onStop} hitSlop={8}>
        <Stop size={13} color={d.red} />
      </Pressable>
    </View>
  );
}

type Props = {
  cwd: string;
  tasks: readonly SessionTask[];
  onStop: (taskId: string) => void;
  onBack: () => void;
};

export function TasksScreen({ cwd, tasks, onStop, onBack }: Props) {
  const insets = useSafeAreaInsets();
  const [filter, setFilter] = useState<Kind | "all">("all");

  const present = SECTIONS.filter((section) => tasks.some((task) => task.kind === section.kind));
  const filters = FILTERS.filter((entry) => entry.key === "all" || present.some((s) => s.kind === entry.key));

  return (
    <View style={[styles.root, { paddingTop: Math.max(insets.top + 8, 52) }]}>
      <View style={styles.header}>
        <Pressable accessibilityLabel="Back to chat" onPress={onBack}>
          <Glass style={styles.circle} interactive>
            <ChevronLeft size={24} color={d.text} />
          </Glass>
        </Pressable>
        <View style={styles.headText}>
          <Text style={styles.title}>Running now</Text>
          <Text style={styles.subtitle} numberOfLines={1}>
            {cwd}
            {tasks.length ? ` · ${count(tasks.length, "task")}` : ""}
          </Text>
        </View>
      </View>

      {filters.length > 2 ? (
        <View style={styles.filters}>
          {filters.map((entry) => (
            <Pressable
              key={entry.key}
              style={[styles.filter, filter === entry.key && styles.filterOn]}
              onPress={() => setFilter(entry.key)}
            >
              <Text style={[styles.filterText, filter === entry.key && styles.filterTextOn]}>{entry.label}</Text>
            </Pressable>
          ))}
        </View>
      ) : null}

      <ScrollView
        contentContainerStyle={[styles.scrollBody, { paddingBottom: Math.max(insets.bottom, 20) }]}
        showsVerticalScrollIndicator={false}
      >
        {present.length ? (
          present
            .filter((section) => filter === "all" || filter === section.kind)
            .map((section) => {
              const rows = tasks.filter((task) => task.kind === section.kind);
              const Icon = section.icon;
              return (
                <View key={section.kind} style={styles.section}>
                  <View style={styles.sectionHead}>
                    <Icon size={13} color={d.muted} />
                    <Text style={styles.sectionLabel}>
                      {section.label} · {rows.length}
                    </Text>
                    {rows.length > 1 ? (
                      <Pressable onPress={() => rows.forEach((task) => onStop(task.taskId))} hitSlop={8}>
                        <Text style={styles.stopAll}>Stop all</Text>
                      </Pressable>
                    ) : null}
                  </View>
                  {rows.map((task) => (
                    <Row key={task.taskId} task={task} onStop={() => onStop(task.taskId)} />
                  ))}
                </View>
              );
            })
        ) : (
          <Text style={styles.empty}>Nothing running</Text>
        )}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: d.bg, paddingHorizontal: 16 },
  header: { flexDirection: "row", alignItems: "center", gap: 12, paddingBottom: 16 },
  circle: { width: 44, height: 44, borderRadius: 22, overflow: "hidden", alignItems: "center", justifyContent: "center" },
  headText: { flex: 1, gap: 2 },
  title: { color: d.text, fontSize: 20, fontFamily: font.semibold },
  subtitle: { color: d.faint, fontSize: 11.5, fontFamily: font.mono },
  filters: { flexDirection: "row", gap: 8, paddingBottom: 12 },
  filter: { paddingHorizontal: 12, paddingVertical: 6, borderRadius: 14, backgroundColor: d.chip },
  filterOn: { backgroundColor: d.greenChip },
  filterText: { color: d.muted, fontSize: 12, fontFamily: font.medium },
  filterTextOn: { color: d.green },
  scrollBody: { gap: 18 },
  empty: { color: d.faint, fontSize: 13, fontFamily: font.mono, textAlign: "center", marginTop: 40 },
  section: { gap: 6 },
  sectionHead: { flexDirection: "row", alignItems: "center", gap: 7, paddingBottom: 2 },
  sectionLabel: { flex: 1, color: d.muted, fontSize: 11.5, fontFamily: font.mono },
  stopAll: { color: d.red, fontSize: 11.5, fontFamily: font.medium },
  row: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    paddingVertical: 10,
    paddingHorizontal: 12,
    borderRadius: 14,
    backgroundColor: d.code,
  },
  status: { width: 16, alignItems: "center" },
  rowBody: { flex: 1, gap: 3 },
  rowTitle: { color: d.text, fontSize: 13, fontFamily: font.monoSemibold },
  rowMeta: { color: d.faint, fontSize: 11, fontFamily: font.mono },
  stop: { width: 26, height: 26, borderRadius: 13, alignItems: "center", justifyContent: "center", backgroundColor: d.delBg },
});
