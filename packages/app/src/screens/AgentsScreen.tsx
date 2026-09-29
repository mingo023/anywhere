import React, { useMemo } from "react";
import { FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import { theme } from "../theme";
import { useSession } from "../session";
import { byUrgency, look } from "../status";

export function AgentsScreen({ onOpen }: { onOpen: (agentId: string) => void }) {
  const { agents, disconnect } = useSession();
  const rows = useMemo(() => [...agents].sort(byUrgency), [agents]);

  return (
    <View style={styles.root}>
      <View style={styles.header}>
        <Text style={styles.heading}>Agents</Text>
        <Pressable onPress={disconnect}>
          <Text style={styles.link}>Disconnect</Text>
        </Pressable>
      </View>

      <FlatList
        data={rows}
        keyExtractor={(a) => a.id}
        contentContainerStyle={styles.list}
        ListEmptyComponent={<Text style={styles.empty}>Start one on your Mac: pocketd run claude</Text>}
        renderItem={({ item }) => {
          const { tone, label } = look(item);
          return (
            <Pressable
              style={[styles.row, !item.attached && styles.detached]}
              disabled={!item.attached}
              onPress={() => onOpen(item.id)}
            >
              <View style={[styles.dot, { backgroundColor: theme[tone] }]} />
              <View style={styles.rowText}>
                <Text style={styles.title} numberOfLines={1}>
                  {item.title}
                </Text>
                <Text style={styles.cwd} numberOfLines={1}>
                  {item.attached ? `${item.provider} · ${item.cwd}` : "Not attached · open on your Mac"}
                </Text>
              </View>
              {label ? <Text style={[styles.label, { color: theme[tone] }]}>{label}</Text> : null}
            </Pressable>
          );
        }}
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
  detached: { opacity: 0.5 },
  dot: { width: 8, height: 8, borderRadius: 4 },
  rowText: { flex: 1 },
  title: { color: theme.text, fontSize: 15 },
  cwd: { color: theme.muted, fontSize: 11, marginTop: 2 },
  label: { fontSize: 12, fontWeight: "600" },
});
