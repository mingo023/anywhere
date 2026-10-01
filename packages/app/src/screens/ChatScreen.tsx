import React, { useEffect, useMemo, useRef, useState } from "react";
import { Animated, AppState, Keyboard, Platform, Pressable, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { d, font } from "../design";
import { useBanner, useSession } from "../session";
import { ChevronLeft, GitBranch } from "../icons";
import { changedFiles, totals } from "../changes";
import { backLabel, needsYouElsewhere } from "../order";
import { ConnectionPill } from "../components/ConnectionPill";
import { Glass } from "../components/Glass";
import { TimelineView, type Pending } from "../components/TimelineView";
import { Composer } from "../components/Composer";
import { ChangesScreen } from "./ChangesScreen";
import { restoreNotice } from "../status";
import { contextChip } from "../context";
import { pending as openFor } from "../permissions";

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

function useActive(): boolean {
  const [active, setActive] = useState(AppState.currentState === "active");

  useEffect(() => {
    const change = AppState.addEventListener("change", (state) => setActive(state === "active"));
    return () => change.remove();
  }, []);

  return active;
}

/** A composer command, not a prompt: it drives the daemon's compaction instead of reaching the model. */
function isCompact(text: string): boolean {
  return /^\/compact$/i.test(text.trim());
}

const providerLabel: Record<string, string> = { claude: "Claude", codex: "Codex" };

function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

export function ChatScreen({ agentId, onBack }: { agentId: string; onBack: () => void }) {
  const {
    link,
    agents,
    timelines,
    permissions,
    error,
    clearError,
    loadTimeline,
    view,
    prompt,
    compact,
    interrupt,
    redial,
    drafts,
  } = useSession();
  const banner = useBanner();
  const insets = useSafeAreaInsets();
  const agent = agents.find((a) => a.id === agentId);
  const elsewhere = useMemo(() => needsYouElsewhere(agents, agentId), [agents, agentId]);
  const provider = providerLabel[agent?.provider ?? ""] ?? "Agent";
  const meta = [agent ? basename(agent.cwd) : null, agent?.model].filter(Boolean).join(" · ");
  const items = timelines[agentId] ?? [];
  const notice = agent && restoreNotice(agent);
  const files = useMemo(() => changedFiles(items), [items]);
  const sum = useMemo(() => totals(files), [files]);
  const [showChanges, setShowChanges] = useState(false);

  const compacting = agent?.compacting === true;
  const busy = agent?.status === "working";
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [headerHeight, setHeaderHeight] = useState(0);
  const [dockHeight, setDockHeight] = useState(0);
  const keyboard = useKeyboard();
  const active = useActive();

  useEffect(() => {
    loadTimeline(agentId);
  }, [agentId, loadTimeline]);

  useEffect(() => {
    if (!active) return;
    view([agentId]);
    return () => view([]);
  }, [agentId, active, view]);

  useEffect(() => {
    setStartedAt(busy ? Date.now() : null);
  }, [busy, agentId]);

  const pending: Pending | undefined =
    startedAt === null
      ? undefined
      : { startedAt, model: agent?.model, waiting: openFor(permissions, agentId).length > 0, compacting };

  return (
    <View style={styles.root}>
      <TimelineView
        items={items}
        pending={pending}
        notice={notice}
        insetTop={headerHeight}
        insetBottom={dockHeight + keyboard.height}
      />

      <View
        style={[styles.header, { paddingTop: Math.max(insets.top + 8, 52) }]}
        pointerEvents="box-none"
        onLayout={(event) => setHeaderHeight(event.nativeEvent.layout.height)}
      >
        <View style={styles.headerRow} pointerEvents="box-none">
          <Pressable accessibilityLabel={backLabel(elsewhere)} onPress={onBack}>
            <Glass style={[styles.circle, elsewhere > 0 && styles.badged]} interactive>
              <ChevronLeft size={24} color={d.text} />
              {elsewhere > 0 ? <Text style={styles.badge}>{elsewhere}</Text> : null}
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

          {files.length ? (
            <Pressable
              accessibilityLabel="Review changes"
              onPress={() => {
                Keyboard.dismiss();
                setShowChanges(true);
              }}
            >
              <Glass style={styles.diff} interactive>
                <GitBranch size={15} color={d.text} />
                <Text style={styles.added}>+{sum.added}</Text>
                <Text style={styles.removed}>−{sum.removed}</Text>
              </Glass>
            </Pressable>
          ) : null}
        </View>

        <ConnectionPill banner={banner} onPress={redial} />

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
          key={agentId}
          placeholder={`Message ${provider}…`}
          working={busy}
          offline={link.state !== "online"}
          initialText={drafts.get(agentId)}
          onChangeText={(text) => drafts.set(agentId, text)}
          provider={agent?.provider ?? ""}
          providerName={provider}
          paddingBottom={keyboard.height ? 12 : Math.max(insets.bottom, 30)}
          onSend={(text) => {
            drafts.clear(agentId);
            if (isCompact(text)) compact(agentId);
            else prompt(agentId, text);
          }}
          onInterrupt={() => interrupt(agentId)}
          context={agent && contextChip(agent)}
        />
      </Animated.View>

      {showChanges ? <ChangesScreen agentId={agentId} onBack={() => setShowChanges(false)} /> : null}
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
  badged: { width: undefined, minWidth: 44, flexDirection: "row", paddingLeft: 6, paddingRight: 14 },
  badge: { color: d.text, fontSize: 15, fontFamily: font.semibold },
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
  added: { color: d.green, fontSize: 12, fontFamily: font.mono },
  removed: { color: d.red, fontSize: 12, fontFamily: font.mono },
});
