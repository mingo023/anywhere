import React from "react";
import { StyleSheet, Text, View } from "react-native";
import type { TaskItem } from "@pocket/protocol";
import { d, font } from "../design";
import { Check, CircleOutline, List } from "../icons";

function State({ status }: { status: TaskItem["status"] }) {
  if (status === "completed") return <Check size={13} color={d.green} />;
  if (status === "in_progress") return <CircleOutline size={13} color={d.teal} />;
  return <CircleOutline size={13} color={d.faint} />;
}

export function TaskList({ items }: { items: readonly TaskItem[] }) {
  const done = items.filter((item) => item.status === "completed").length;

  return (
    <View style={styles.card}>
      <View style={styles.header}>
        <List size={15} color={d.faint} />
        <Text style={styles.title}>Tasks</Text>
        <Text style={styles.progress}>
          {done}/{items.length}
        </Text>
      </View>
      <View style={styles.body}>
        {items.map((item, index) => (
          <View key={index} style={styles.row}>
            <View style={styles.state}>
              <State status={item.status} />
            </View>
            <Text
              style={[
                styles.text,
                item.status === "completed" && styles.done,
                item.status === "in_progress" && styles.active,
              ]}
            >
              {item.text}
            </Text>
          </View>
        ))}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: { backgroundColor: d.card, borderWidth: 1, borderColor: d.cardRule, borderRadius: 14, overflow: "hidden" },
  header: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    height: 40,
    paddingHorizontal: 12,
    borderBottomWidth: 1,
    borderBottomColor: d.cardRule,
  },
  title: { flex: 1, color: d.text, fontSize: 12.5, fontFamily: font.monoSemibold },
  progress: {
    color: d.muted,
    fontSize: 11,
    fontFamily: font.mono,
    backgroundColor: d.chip,
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 10,
    overflow: "hidden",
  },
  body: { paddingVertical: 6 },
  row: { flexDirection: "row", gap: 10, paddingHorizontal: 12, paddingVertical: 4 },
  state: { width: 14, paddingTop: 3 },
  text: { flex: 1, color: d.muted, fontSize: 13.5, lineHeight: 20, fontFamily: font.regular },
  done: { color: d.faint, textDecorationLine: "line-through" },
  active: { color: d.text },
});
