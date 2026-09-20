import React, { useEffect, useMemo, useRef, useState } from "react";
import { Animated, Keyboard, Platform, Pressable, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { d, font } from "../design";
import type { TimelineItem } from "@pocket/protocol";
import { useSession } from "../session";
import { Bot, ChevronLeft, GitBranch, Terminal } from "../icons";
import { Glass } from "../components/Glass";
import { TimelineView, type Pending } from "../components/TimelineView";
import { TasksScreen } from "./TasksScreen";
import { Composer } from "../components/Composer";

/** The composer floats over the list, so it has to ride the keyboard itself instead of relying on padding. */
function useKeyboard() {
  const [height, setHeight] = useState(0);
  const offset = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    const ios = Platform.OS === "ios";
    const slide = (to: number, duration: number) =>
      Animated.timing(offset, { toValue: to, duration: duration || 250, useNativeDriver: true }).start();

    const show = Keyboard.addListener(ios ? "keyboardWillShow" : "keyboardDidShow", (event) => {
      setHeight(event.endCoordinates.height);
      slide(-event.endCoordinates.height, event.duration);
    });
    const hide = Keyboard.addListener(ios ? "keyboardWillHide" : "keyboardDidHide", (event) => {
      setHeight(0);
      slide(0, event.duration);
    });
    return () => {
      show.remove();
      hide.remove();
    };
  }, [offset]);

  return { height, offset };
}

/** A composer command, not a prompt: it drives the daemon's compaction instead of reaching the model. */
function isCompact(text: string): boolean {
  return /^\/compact$/i.test(text.trim());
}

function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

/** Latest edit per file, so re-touching one file does not count its lines twice. */
function diffTotals(items: readonly TimelineItem[]): { added: number; removed: number } {
  const byPath = new Map<string, { added: number; removed: number }>();
  for (const item of items) {
    if (item.kind !== "tool") continue;
    const detail = item.call.detail;
    if ((detail.kind !== "edit" && detail.kind !== "write") || !detail.diff) continue;
    byPath.set(detail.path, { added: detail.diff.additions, removed: detail.diff.deletions });
  }
  let added = 0;
  let removed = 0;
  for (const entry of byPath.values()) {
    added += entry.added;
    removed += entry.removed;
  }
  return { added, removed };
}

export function ChatScreen({ agentId, onBack }: { agentId: string; onBack: () => void }) {
  const { agents, profiles, timelines, tasks, permission, error, clearError, loadTimeline, prompt, compact, interrupt, stopTask } =
    useSession();
  const insets = useSafeAreaInsets();
  const agent = agents.find((a) => a.id === agentId);
  const provider = profiles.find((p) => p.id === agent?.profileId)?.label ?? "Agent";
  const meta = [agent ? basename(agent.cwd) : null, agent?.model].filter(Boolean).join(" · ");
  const items = timelines[agentId] ?? [];
  const diff = useMemo(() => diffTotals(items), [items]);
  const liveTasks = (tasks[agentId] ?? []).filter((task) => task.status === "running");

  const compacting = agent?.status === "compacting";
  const busy = agent?.status === "running" || compacting;
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [tasksOpen, setTasksOpen] = useState(false);
  const [headerHeight, setHeaderHeight] = useState(0);
  const [dockHeight, setDockHeight] = useState(0);
  const keyboard = useKeyboard();

  useEffect(() => {
    loadTimeline(agentId);
  }, [agentId, loadTimeline]);

  useEffect(() => {
    setStartedAt(busy ? Date.now() : null);
  }, [busy, agentId]);

  const pending: Pending | undefined =
    startedAt === null
      ? undefined
      : { startedAt, model: agent?.model, waiting: permission?.agentId === agentId, compacting };

  if (tasksOpen) {
    return (
      <TasksScreen
        cwd={agent?.cwd ?? ""}
        tasks={liveTasks}
        onStop={(taskId) => stopTask(agentId, taskId)}
        onBack={() => setTasksOpen(false)}
      />
    );
  }

  return (
    <View style={styles.root}>
      <TimelineView
        items={items}
        pending={pending}
        insetTop={headerHeight}
        insetBottom={dockHeight + keyboard.height}
      />

      <View
        style={[styles.header, { paddingTop: Math.max(insets.top + 8, 52) }]}
        pointerEvents="box-none"
        onLayout={(event) => setHeaderHeight(event.nativeEvent.layout.height)}
      >
        <View style={styles.headerRow} pointerEvents="box-none">
          <Pressable accessibilityLabel="Back to agents" onPress={onBack}>
            <Glass style={styles.circle} interactive>
              <ChevronLeft size={24} color={d.text} />
            </Glass>
          </Pressable>

          <Glass style={styles.titlePill}>
            <Text style={styles.title} numberOfLines={1}>
              {agent?.title ?? "Agent"}
            </Text>
            <Text style={styles.subtitle} numberOfLines={1}>
              <Text style={styles.provider}>{provider}</Text>
              {meta ? ` · ${meta}` : ""}
            </Text>
          </Glass>

          {liveTasks.length ? (
            <Pressable accessibilityLabel="Running tasks" onPress={() => setTasksOpen(true)}>
              <Glass style={styles.tasks} interactive>
                <Bot size={15} color={d.green} />
                <Text style={styles.tasksCount}>{liveTasks.length}</Text>
              </Glass>
            </Pressable>
          ) : null}

          <Pressable accessibilityLabel="Review changes">
            <Glass style={styles.diff} interactive>
              <GitBranch size={15} color={d.text} />
              <Text style={styles.added}>+{diff.added}</Text>
              <Text style={styles.removed}>−{diff.removed}</Text>
            </Glass>
          </Pressable>

          <Pressable accessibilityLabel="Open raw terminal">
            <Glass style={styles.circle} interactive>
              <Terminal size={20} color={d.text} />
            </Glass>
          </Pressable>
        </View>

        {error ? (
          <Pressable style={styles.banner} accessibilityLabel="Dismiss error" onPress={clearError}>
            <Text style={styles.bannerText}>{error}</Text>
          </Pressable>
        ) : null}
      </View>

      <Animated.View
        style={[styles.dock, { transform: [{ translateY: keyboard.offset }] }]}
        onLayout={(event) => setDockHeight(event.nativeEvent.layout.height)}
      >
        <Composer
          placeholder={`Message ${provider}…`}
          busy={busy}
          paddingBottom={keyboard.height ? 12 : Math.max(insets.bottom, 30)}
          onSend={(text) => (isCompact(text) ? compact(agentId) : prompt(agentId, text))}
          onInterrupt={() => interrupt(agentId)}
        />
      </Animated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: d.bg },
  header: { position: "absolute", top: 0, left: 0, right: 0 },
  headerRow: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingBottom: 10 },
  dock: { position: "absolute", left: 0, right: 0, bottom: 0 },
  circle: {
    width: 44,
    height: 44,
    borderRadius: 22,
    overflow: "hidden",
    alignItems: "center",
    justifyContent: "center",
  },
  titlePill: {
    flex: 1,
    height: 44,
    borderRadius: 22,
    overflow: "hidden",
    paddingHorizontal: 14,
    justifyContent: "center",
    gap: 1,
  },
  banner: {
    marginHorizontal: 12,
    marginBottom: 10,
    paddingHorizontal: 14,
    paddingVertical: 10,
    borderRadius: 14,
    backgroundColor: d.delBg,
  },
  bannerText: { color: d.delText, fontSize: 12.5, fontFamily: font.mono },
  title: { color: d.text, fontSize: 15, fontFamily: font.semibold },
  subtitle: { color: d.muted, fontSize: 11, fontFamily: font.mono },
  provider: { color: d.teal },
  diff: {
    height: 44,
    paddingHorizontal: 12,
    borderRadius: 22,
    overflow: "hidden",
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
  },
  tasks: {
    height: 44,
    paddingHorizontal: 12,
    borderRadius: 22,
    overflow: "hidden",
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
  },
  tasksCount: { color: d.green, fontSize: 12, fontFamily: font.mono },
  added: { color: d.green, fontSize: 12, fontFamily: font.mono },
  removed: { color: d.red, fontSize: 12, fontFamily: font.mono },
});
