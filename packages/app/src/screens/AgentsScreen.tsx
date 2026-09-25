import React from "react";
import { FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import type { AgentSummary } from "@pocket/protocol";
import { theme } from "../theme";
import { useSession } from "../session";

const statusColor: Record<AgentSummary["status"], string> = {
  initializing: theme.warn,
  idle: theme.ok,
  running: theme.accent,
  compacting: theme.accent,
  error: theme.error,
  closed: theme.muted,
};

export function AgentsScreen({ onOpen }: { onOpen: (agentId: string) => void }) {
  const { agents, disconnect } = useSession();

  return (
    <View style={styles.root}>
      <View style={styles.header}>
        <Text style={styles.heading}>Agents</Text>
        <Pressable onPress={disconnect}>
          <Text style={styles.link}>Disconnect</Text>
        </Pressable>
      </View>

      <FlatList
        data={agents}
        keyExtractor={(a) => a.id}
        contentContainerStyle={styles.list}
        ListEmptyComponent={<Text style={styles.empty}>Start one on your Mac: pocketd run claude</Text>}
        renderItem={({ item }) => (
          <Pressable style={styles.row} onPress={() => onOpen(item.id)}>
            <View style={[styles.dot, { backgroundColor: statusColor[item.status] }]} />
            <View style={styles.rowText}>
              <Text style={styles.title} numberOfLines={1}>
                {item.title}
              </Text>
              <Text style={styles.cwd} numberOfLines={1}>
                {item.provider} · {item.cwd}
              </Text>
            </View>
          </Pressable>
        )}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
  header: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingHorizontal: theme.gap,
    paddingVertical: 12,
  },
  heading: { color: theme.text, fontSize: 22, fontWeight: "700" },
  link: { color: theme.accent, fontSize: 13 },
  list: { paddingHorizontal: theme.gap, gap: 8 },
  empty: { color: theme.muted, textAlign: "center", marginTop: 40 },
  row: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    backgroundColor: theme.surface,
    borderWidth: 1,
    borderColor: theme.border,
    borderRadius: theme.radius,
    padding: 14,
  },
  dot: { width: 8, height: 8, borderRadius: 4 },
  rowText: { flex: 1 },
  title: { color: theme.text, fontSize: 15 },
  cwd: { color: theme.muted, fontSize: 11, marginTop: 2 },
});
