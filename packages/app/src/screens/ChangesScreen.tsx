import React, { useEffect, useMemo, useRef, useState } from "react";
import { Animated, FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { d, font } from "../design";
import { useSession } from "../session";
import { changedFiles, changeRows, totals, type ChangeRow, type ChangedFile } from "../changes";
import { ChevronLeft, ChevronRight } from "../icons";
import { Glass } from "../components/Glass";
import { DiffView } from "../components/DiffView";

function FileRow({ file, open, onToggle }: { file: ChangedFile; open: boolean; onToggle: () => void }) {
  const turn = useRef(new Animated.Value(open ? 1 : 0)).current;

  useEffect(() => {
    Animated.timing(turn, { toValue: open ? 1 : 0, duration: 200, useNativeDriver: true }).start();
  }, [open, turn]);

  const rotate = turn.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "90deg"] });

  return (
    <Pressable style={styles.file} onPress={onToggle}>
      <Animated.View style={{ transform: [{ rotate }] }}>
        <ChevronRight size={16} color={d.muted} />
      </Animated.View>
      <Text style={styles.path} numberOfLines={1}>
        {file.path}
      </Text>
      <Text style={styles.added}>+{file.additions}</Text>
      <Text style={styles.removed}>−{file.deletions}</Text>
    </Pressable>
  );
}

export function ChangesScreen({ agentId, onBack }: { agentId: string; onBack: () => void }) {
  const { timelines, hasOlder } = useSession();
  const insets = useSafeAreaInsets();
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const files = useMemo(() => changedFiles(timelines[agentId] ?? []), [timelines, agentId]);
  const rows = useMemo(() => changeRows(files, open), [files, open]);
  const sum = totals(files);
  const count = `${files.length} ${files.length === 1 ? "file" : "files"}`;

  const toggle = (path: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (!next.delete(path)) next.add(path);
      return next;
    });

  const renderItem = ({ item }: { item: ChangeRow }) =>
    item.kind === "file" ? (
      <FileRow file={item.file} open={item.open} onToggle={() => toggle(item.file.path)} />
    ) : (
      <View style={styles.edit}>
        <Text style={styles.editLabel}>
          Edit {item.index + 1} of {item.count}
        </Text>
        <DiffView diff={item.diff} />
      </View>
    );

  return (
    <View style={[styles.root, { paddingTop: Math.max(insets.top + 8, 52) }]}>
      <View style={styles.header}>
        <Pressable accessibilityLabel="Back to session" onPress={onBack}>
          <Glass style={styles.circle} interactive>
            <ChevronLeft size={24} color={d.text} />
          </Glass>
        </Pressable>
        <Text style={styles.title}>Changes</Text>
        <Text style={styles.added}>+{sum.added}</Text>
        <Text style={styles.removed}>−{sum.removed}</Text>
      </View>
      <Text style={styles.subtitle}>
        {hasOlder[agentId] ? "Recent edits" : "Edits in this session"} · {count}
      </Text>
      <FlatList
        data={rows}
        keyExtractor={(row) => row.key}
        renderItem={renderItem}
        contentContainerStyle={{ paddingBottom: insets.bottom + 16 }}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  root: { position: "absolute", top: 0, right: 0, bottom: 0, left: 0, backgroundColor: d.bg },
  header: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingBottom: 6 },
  circle: {
    width: 44,
    height: 44,
    borderRadius: 22,
    overflow: "hidden",
    alignItems: "center",
    justifyContent: "center",
  },
  title: { flex: 1, color: d.text, fontSize: 17, fontFamily: font.semibold, textAlign: "center" },
  subtitle: { color: d.muted, fontSize: 12, fontFamily: font.regular, paddingHorizontal: 16, paddingBottom: 8 },
  file: {
    height: 44,
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    paddingHorizontal: 16,
    borderTopWidth: 1,
    borderTopColor: d.rule,
  },
  path: { flex: 1, color: d.text, fontSize: 13, fontFamily: font.mono },
  added: { color: d.green, fontSize: 12, fontFamily: font.mono },
  removed: { color: d.red, fontSize: 12, fontFamily: font.mono },
  edit: { paddingBottom: 8 },
  editLabel: { color: d.faint, fontSize: 11, fontFamily: font.mono, paddingHorizontal: 16, paddingVertical: 6 },
});
