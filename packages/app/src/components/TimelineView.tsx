import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  Animated,
  Easing,
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  View,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
} from "react-native";
import type { TimelineItem, ToolCall } from "@pocket/protocol";
import { d, font } from "../design";
import { Asterisk, ChevronDown, Minus } from "../icons";
import { Glass } from "./Glass";
import { Markdown } from "./Markdown";
import { Shimmer } from "./Shimmer";
import { PlanCard } from "./PlanCard";
import { TaskList } from "./TaskList";
import { ToolGroup } from "./ToolGroup";

type Item = Exclude<TimelineItem, { kind: "tool" }>;

type Row =
  | { key: string; kind: "group"; calls: ToolCall[] }
  | { key: string; kind: "item"; item: Item };

function toRows(items: readonly TimelineItem[]): Row[] {
  const rows: Row[] = [];
  for (const item of items) {
    if (item.kind !== "tool") {
      rows.push({ key: item.id, kind: "item", item });
      continue;
    }
    const last = rows[rows.length - 1];
    if (last?.kind === "group") last.calls.push(item.call);
    else rows.push({ key: item.id, kind: "group", calls: [item.call] });
  }
  return rows;
}

function summarise(text: string): string {
  const line = text.split("\n").find((candidate) => candidate.trim());
  if (!line) return "Thinking";
  const clean = line.replace(/^#+\s*/, "").replace(/[*_`]/g, "").trim();
  return clean.length > 90 ? `${clean.slice(0, 90)}…` : clean;
}

export type Pending = { startedAt: number; model?: string | undefined; waiting: boolean; compacting: boolean };

export function elapsed(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  return total < 60 ? `${total}s` : `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, "0")}s`;
}

export function useTick(startedAt: number): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [startedAt]);
  return now - startedAt;
}

function Working({ startedAt, model, waiting, compacting }: Pending) {
  const ms = useTick(startedAt);
  const pulse = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(pulse, { toValue: 1, duration: 800, easing: Easing.inOut(Easing.quad), useNativeDriver: true }),
        Animated.timing(pulse, { toValue: 0, duration: 800, easing: Easing.inOut(Easing.quad), useNativeDriver: true }),
      ]),
    );
    loop.start();
    return () => loop.stop();
  }, [pulse]);

  const label = waiting
    ? "Waiting for approval"
    : compacting
      ? `Compacting context ${elapsed(ms)}`
      : [`Working ${elapsed(ms)}`, model?.replace(/^claude-/, "")].filter(Boolean).join(" · ");

  return (
    <View style={styles.workingRow}>
      <Animated.View style={{ opacity: pulse.interpolate({ inputRange: [0, 1], outputRange: [0.35, 1] }) }}>
        <Asterisk size={15} color={waiting ? d.teal : d.green} />
      </Animated.View>
      <Shimmer style={styles.workingText} base={d.faint} highlight={d.text}>
        {label}
      </Shimmer>
    </View>
  );
}

function tokens(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(1)}k` : String(value);
}

function Thinking({ text }: { text: string }) {
  const [open, setOpen] = useState(false);

  return (
    <View>
      <Pressable style={styles.thinkingHead} onPress={() => setOpen((v) => !v)}>
        <Minus size={14} color={d.faint} />
        <Text style={styles.thinkingText} numberOfLines={1}>
          {summarise(text)}
        </Text>
      </Pressable>
      {open ? (
        <View style={styles.thinkingBody}>
          <Markdown text={text} dim />
        </View>
      ) : null}
    </View>
  );
}

function Result({ item }: { item: Extract<TimelineItem, { kind: "result" }> }) {
  if (!item.ok) return <Text style={[styles.paragraph, { color: d.red }]}>{item.error ?? "Run failed."}</Text>;

  const metrics = [
    item.usage ? `${tokens(item.usage.inputTokens)} in · ${tokens(item.usage.outputTokens)} out` : null,
    item.turns ? `${item.turns} turn${item.turns === 1 ? "" : "s"}` : null,
  ].filter(Boolean);

  return (
    <View style={styles.resultRow}>
      <Text style={styles.paragraph}>
        Done in <Text style={styles.accent}>{(item.durationMs / 1000).toFixed(1)}s</Text>.
      </Text>
      {metrics.length ? <Text style={styles.metrics}>{metrics.join(" · ")}</Text> : null}
    </View>
  );
}

function Compacted({ trigger }: Extract<TimelineItem, { kind: "compact" }>) {
  return (
    <View style={styles.compactRow}>
      <View style={styles.rule} />
      <Text style={styles.compactText}>{trigger === "auto" ? "Auto-compacted" : "Context compacted"}</Text>
      <View style={styles.rule} />
    </View>
  );
}

function Item({ item }: { item: Item }) {
  switch (item.kind) {
    case "assistant":
      return <Markdown text={item.text} />;
    case "thinking":
      return <Thinking text={item.text} />;
    case "tasks":
      return <TaskList items={item.items} />;
    case "plan":
      return <PlanCard text={item.text} />;
    case "compact":
      return <Compacted {...item} />;
    case "user":
      return (
        <View style={styles.bubble}>
          <Text style={styles.bubbleText}>{item.text}</Text>
        </View>
      );
    case "result":
      return <Result item={item} />;
  }
}

const BOTTOM_SLACK = 32;

type Props = {
  items: readonly TimelineItem[];
  pending?: Pending;
  insetTop: number;
  insetBottom: number;
};

export function TimelineView({ items, pending, insetTop, insetBottom }: Props) {
  const rows = useMemo(() => toRows(items), [items]);
  const list = useRef<FlatList<Row>>(null);
  const [atBottom, setAtBottom] = useState(true);
  /** onContentSizeChange fires outside render, so it needs the live value rather than a captured one. */
  const pinned = useRef(true);
  const content = useRef(0);
  const viewport = useRef(0);

  const onScroll = ({ nativeEvent }: NativeSyntheticEvent<NativeScrollEvent>) => {
    const { contentOffset, contentSize, layoutMeasurement } = nativeEvent;
    content.current = contentSize.height;
    viewport.current = layoutMeasurement.height;
    const bottom = contentSize.height - layoutMeasurement.height - contentOffset.y <= BOTTOM_SLACK;
    pinned.current = bottom;
    setAtBottom(bottom);
  };

  /**
   * VirtualizedList calls onContentSizeChange before refreshing its own contentLength, so scrollToEnd
   * there aims at the previous height and stops short by whatever just grew.
   */
  const toBottom = () => {
    list.current?.scrollToOffset({ offset: Math.max(0, content.current - viewport.current), animated: true });
  };

  return (
    <View style={styles.list}>
      <FlatList
        ref={list}
        data={rows}
        keyExtractor={(row) => row.key}
        renderItem={({ item }) => (item.kind === "group" ? <ToolGroup calls={item.calls} /> : <Item item={item.item} />)}
        contentContainerStyle={[styles.content, { paddingTop: insetTop + 18, paddingBottom: insetBottom + 10 }]}
        ListFooterComponent={pending ? <Working {...pending} /> : null}
        showsVerticalScrollIndicator={false}
        keyboardDismissMode="interactive"
        keyboardShouldPersistTaps="handled"
        scrollEventThrottle={16}
        onScroll={onScroll}
        onLayout={(event) => {
          viewport.current = event.nativeEvent.layout.height;
        }}
        onContentSizeChange={(_, height) => {
          content.current = height;
          if (pinned.current) toBottom();
        }}
      />
      {atBottom ? null : (
        <Pressable
          style={[styles.toBottom, { bottom: insetBottom + 12 }]}
          accessibilityLabel="Scroll to latest"
          onPress={toBottom}
        >
          <Glass style={styles.toBottomSurface} interactive>
            <ChevronDown size={26} color={d.text} />
          </Glass>
        </Pressable>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { flex: 1 },
  toBottom: { position: "absolute", right: 16 },
  toBottomSurface: {
    width: 46,
    height: 46,
    borderRadius: 23,
    overflow: "hidden",
    alignItems: "center",
    justifyContent: "center",
  },
  content: { paddingHorizontal: 16, gap: 16 },
  paragraph: { color: d.body, fontSize: 15, lineHeight: 23, fontFamily: font.regular },
  accent: { color: d.green, fontSize: 14, fontFamily: font.mono },
  resultRow: { flexDirection: "row", alignItems: "baseline", gap: 10 },
  metrics: { color: d.faint, fontSize: 11.5, fontFamily: font.mono },
  compactRow: { flexDirection: "row", alignItems: "center", gap: 10 },
  rule: { flex: 1, height: StyleSheet.hairlineWidth, backgroundColor: d.faint },
  compactText: { color: d.faint, fontSize: 11.5, fontFamily: font.mono },
  workingRow: { flexDirection: "row", alignItems: "center", gap: 8 },
  workingText: { color: d.faint, fontSize: 14, fontFamily: font.regular },
  thinkingHead: { flexDirection: "row", alignItems: "center", gap: 8 },
  thinkingText: { flex: 1, color: d.faint, fontSize: 14, fontFamily: font.regular },
  thinkingBody: { paddingTop: 8, paddingLeft: 22 },
  bubble: {
    alignSelf: "flex-end",
    backgroundColor: d.bubble,
    paddingVertical: 10,
    paddingHorizontal: 16,
    borderTopLeftRadius: 18,
    borderTopRightRadius: 18,
    borderBottomRightRadius: 6,
    borderBottomLeftRadius: 18,
  },
  bubbleText: { color: d.bubbleText, fontSize: 15, fontFamily: font.medium },
});
