import React from "react";
import { ScrollView, StyleSheet, Text, View } from "react-native";
import type { DiffLine, FileDiff } from "@pocket/protocol";
import { d, font } from "../design";

const MARK = { add: "+", del: "-", context: " " } as const;

function Line({ line }: { line: DiffLine }) {
  return (
    <View style={[styles.line, line.kind === "add" && styles.addRow, line.kind === "del" && styles.delRow]}>
      <Text style={styles.number}>{line.number ?? ""}</Text>
      <Text
        style={[
          styles.text,
          line.kind === "add" && { color: d.addText },
          line.kind === "del" && { color: d.delText },
        ]}
      >
        {`${MARK[line.kind]}  ${line.text}`}
      </Text>
    </View>
  );
}

export function DiffView({ diff }: { diff: FileDiff }) {
  if (!diff.lines.length) return <Text style={styles.empty}>Empty file</Text>;

  return (
    <View style={styles.root}>
      <ScrollView horizontal showsHorizontalScrollIndicator={false}>
        <View>
          {diff.lines.map((line, index) => (
            <Line key={index} line={line} />
          ))}
        </View>
      </ScrollView>
    </View>
  );
}

export function DiffStat({ diff }: { diff: FileDiff }) {
  return (
    <Text style={styles.stat}>
      {diff.additions ? <Text style={{ color: d.green }}>+{diff.additions}</Text> : null}
      {diff.additions && diff.deletions ? " " : null}
      {diff.deletions ? <Text style={{ color: d.red }}>−{diff.deletions}</Text> : null}
    </Text>
  );
}

const styles = StyleSheet.create({
  root: { backgroundColor: d.code, paddingVertical: 6 },
  line: { flexDirection: "row", gap: 10, paddingHorizontal: 12, paddingVertical: 1 },
  addRow: { backgroundColor: d.addBg },
  delRow: { backgroundColor: d.delBg },
  number: {
    width: 26,
    textAlign: "right",
    color: d.lineNo,
    fontSize: 12,
    lineHeight: 20,
    fontFamily: font.mono,
  },
  text: { color: d.muted, fontSize: 12, lineHeight: 20, fontFamily: font.mono },
  empty: { color: d.faint, fontSize: 12, fontFamily: font.mono, paddingHorizontal: 12, paddingVertical: 8 },
  stat: { fontSize: 11.5, fontFamily: font.mono },
});
