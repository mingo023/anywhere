import React, { useState } from "react";
import { FlatList, Modal, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
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
  const { agents, createAgent, disconnect } = useSession();
  const [composing, setComposing] = useState(false);
  const [cwd, setCwd] = useState("");
  const [prompt, setPrompt] = useState("");

  const start = () => {
    if (!cwd.trim() || !prompt.trim()) return;
    createAgent(cwd.trim(), prompt.trim(), "claude-default");
    setComposing(false);
    setPrompt("");
  };

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
        ListEmptyComponent={<Text style={styles.empty}>No agents yet</Text>}
        renderItem={({ item }) => (
          <Pressable style={styles.row} onPress={() => onOpen(item.id)}>
            <View style={[styles.dot, { backgroundColor: statusColor[item.status] }]} />
            <View style={styles.rowText}>
              <Text style={styles.title} numberOfLines={1}>
                {item.title}
              </Text>
              <Text style={styles.cwd} numberOfLines={1}>
                {item.cwd}
              </Text>
            </View>
          </Pressable>
        )}
      />

      <Pressable style={styles.fab} onPress={() => setComposing(true)}>
        <Text style={styles.fabText}>＋</Text>
      </Pressable>

      <Modal visible={composing} transparent animationType="slide">
        <View style={styles.backdrop}>
          <View style={styles.sheet}>
            <Text style={styles.sheetTitle}>New agent</Text>
            <TextInput
              style={styles.input}
              value={cwd}
              onChangeText={setCwd}
              placeholder="/Users/you/project"
              placeholderTextColor={theme.muted}
              autoCapitalize="none"
              autoCorrect={false}
            />
            <TextInput
              style={[styles.input, styles.multiline]}
              value={prompt}
              onChangeText={setPrompt}
              placeholder="First message"
              placeholderTextColor={theme.muted}
              multiline
            />
            <View style={styles.actions}>
              <Pressable style={[styles.button, styles.cancel]} onPress={() => setComposing(false)}>
                <Text style={styles.cancelText}>Cancel</Text>
              </Pressable>
              <Pressable style={[styles.button, styles.start]} onPress={start}>
                <Text style={styles.startText}>Start</Text>
              </Pressable>
            </View>
          </View>
        </View>
      </Modal>
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
  fab: {
    position: "absolute",
    right: 20,
    bottom: 32,
    width: 56,
    height: 56,
    borderRadius: 28,
    backgroundColor: theme.accent,
    alignItems: "center",
    justifyContent: "center",
  },
  fabText: { color: theme.bg, fontSize: 28, lineHeight: 32 },
  backdrop: { flex: 1, justifyContent: "flex-end", backgroundColor: "#0008" },
  sheet: {
    backgroundColor: theme.surface,
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    padding: 20,
    paddingBottom: 40,
    gap: 12,
  },
  sheetTitle: { color: theme.text, fontSize: 16, fontWeight: "600" },
  input: {
    color: theme.text,
    backgroundColor: theme.surfaceAlt,
    borderWidth: 1,
    borderColor: theme.border,
    borderRadius: theme.radius,
    padding: 12,
    fontSize: 14,
  },
  multiline: { minHeight: 80, textAlignVertical: "top" },
  actions: { flexDirection: "row", gap: 12 },
  button: { flex: 1, borderRadius: theme.radius, paddingVertical: 14, alignItems: "center" },
  cancel: { backgroundColor: theme.surfaceAlt, borderWidth: 1, borderColor: theme.border },
  start: { backgroundColor: theme.accent },
  cancelText: { color: theme.text, fontWeight: "600" },
  startText: { color: theme.bg, fontWeight: "600" },
});
