import React, { useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { d, font } from "../design";
import { Bulb, ChevronDown, ChevronRight } from "../icons";
import { Markdown } from "./Markdown";

function title(text: string): string {
  const heading = text.split("\n").find((line) => line.trim());
  return heading ? heading.replace(/^#+\s*/, "").trim() : "Plan";
}

export function PlanCard({ text }: { text: string }) {
  const [open, setOpen] = useState(false);

  return (
    <View style={styles.card}>
      <Pressable style={styles.header} onPress={() => setOpen((v) => !v)}>
        <Bulb size={15} color={d.green} />
        <Text style={styles.title} numberOfLines={open ? undefined : 1}>
          {title(text)}
        </Text>
        {open ? <ChevronDown size={16} color={d.muted} /> : <ChevronRight size={16} color={d.muted} />}
      </Pressable>
      {open ? (
        <View style={styles.body}>
          <Markdown text={text} />
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  card: { backgroundColor: d.card, borderWidth: 1, borderColor: d.cardRule, borderRadius: 14, overflow: "hidden" },
  header: { flexDirection: "row", alignItems: "center", gap: 10, minHeight: 44, paddingHorizontal: 12 },
  title: { flex: 1, color: d.text, fontSize: 14, fontFamily: font.medium },
  body: { paddingHorizontal: 12, paddingBottom: 12, borderTopWidth: 1, borderTopColor: d.cardRule, paddingTop: 12 },
});
