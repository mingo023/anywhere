import React, { useMemo } from "react";
import { ActivityIndicator, Pressable, SectionList, StyleSheet, Text, View } from "react-native";
import type { AgentSummary } from "@pocket/protocol";
import type { Banner } from "../connectivity";
import { theme } from "../theme";
import { useBanner, useSession } from "../session";
import { look, RANK, restoreNotice } from "../status";
import { allSessions, summary, upNext } from "../order";
import { Check } from "../icons";
import { ConnectionPill } from "../components/ConnectionPill";

function Glyph({ agent }: { agent: AgentSummary }) {
  const { rank, tone } = look(agent);
  if (rank === RANK.done) return <Check size={12} color={theme[tone]} />;
  if (rank === RANK.working) return <ActivityIndicator size="small" color={theme[tone]} style={styles.spinner} />;
  if (rank === RANK.idle) return <View style={styles.glyph} />;
  return <View style={[styles.dot, { backgroundColor: theme[tone] }]} />;
}

function FirstConnect({ banner, onRetry }: { banner: Banner | null; onRetry: () => void }) {
  if (banner?.kind === "unreachable") {
    return (
      <View style={styles.centre}>
        <Text style={styles.centreText}>{banner.text}</Text>
        <Pressable style={styles.retry} onPress={onRetry}>
          <Text style={styles.link}>Try again</Text>
        </Pressable>
      </View>
    );
  }

  return (
    <View style={styles.centre}>
      <ActivityIndicator color={theme.muted} />
      <Text style={styles.centreText}>{banner?.text}</Text>
    </View>
  );
}

export function AgentsScreen({ onOpen, onNew }: { onOpen: (agentId: string) => void; onNew: () => void }) {
  const { link, agents, disconnect, redial } = useSession();
  const banner = useBanner();
  const online = link.state === "online";
  const sections = useMemo(() => {
    if (!agents.length) return [];
    const up = upNext(agents);
    const all = { key: "all", title: "All sessions", data: allSessions(agents) };
    return up.length ? [{ key: "up", title: "Up next", data: up }, all] : [all];
  }, [agents]);
  const counts = useMemo(() => summary(agents), [agents]);

  return (
    <View style={styles.root}>
      <View style={styles.header}>
        <Text style={styles.heading}>Sessions</Text>
        <View style={styles.actions}>
          <Pressable onPress={onNew} disabled={!online} accessibilityState={{ disabled: !online }}>
            <Text style={[styles.link, !online && styles.dim]}>New session</Text>
          </Pressable>
          <Pressable onPress={disconnect}>
            <Text style={styles.link}>Disconnect</Text>
          </Pressable>
        </View>
      </View>
      {counts ? <Text style={styles.summary}>{counts}</Text> : null}

      {link.everOnline ? (
        <>
          <ConnectionPill banner={banner} onPress={redial} />
          <SectionList
            sections={sections}
            keyExtractor={(a) => a.id}
            stickySectionHeadersEnabled={false}
            renderSectionHeader={({ section }) => <Text style={styles.section}>{section.title}</Text>}
            contentContainerStyle={styles.list}
            ListEmptyComponent={
              <View style={styles.empty}>
                <Text style={styles.emptyTitle}>No sessions yet</Text>
                <Text style={styles.emptyText}>Start one on your Mac.</Text>
              </View>
            }
            renderItem={({ item }) => {
              const { tone, label } = look(item);
              const notice = restoreNotice(item);
              return (
                <Pressable
                  style={[styles.row, !item.attached && styles.detached]}
                  disabled={!item.attached}
                  onPress={() => onOpen(item.id)}
                >
                  <Glyph agent={item} />
                  <View style={styles.rowText}>
                    <Text style={styles.title} numberOfLines={1}>
                      {item.title}
                    </Text>
                    <Text style={styles.cwd} numberOfLines={1}>
                      {item.attached ? `${item.provider} · ${item.cwd}` : "Not attached · open on your Mac"}
                    </Text>
                    {notice ? (
                      <Text style={styles.cwd} numberOfLines={1}>
                        {notice}
                      </Text>
                    ) : null}
                  </View>
                  {label ? <Text style={[styles.label, { color: theme[tone] }]}>{label}</Text> : null}
                </Pressable>
              );
            }}
          />
        </>
      ) : (
        <FirstConnect banner={banner} onRetry={redial} />
      )}
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
  actions: { flexDirection: "row", gap: 16 },
  dim: { opacity: 0.4 },
  list: { paddingHorizontal: theme.gap, gap: 8 },
  empty: { alignItems: "center", marginTop: 40, gap: 4 },
  emptyTitle: { color: theme.text, fontSize: 15, fontWeight: "600" },
  emptyText: { color: theme.muted },
  centre: { flex: 1, alignItems: "center", justifyContent: "center", gap: 12, paddingHorizontal: 32 },
  centreText: { color: theme.muted, textAlign: "center" },
  retry: { paddingVertical: 8, paddingHorizontal: 16 },
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
  summary: { color: theme.muted, fontSize: 13, paddingHorizontal: theme.gap, marginTop: -8, marginBottom: 8 },
  section: { color: theme.muted, fontSize: 11, fontWeight: "600", textTransform: "uppercase", marginTop: 12 },
  glyph: { width: 12 },
  dot: { width: 7, height: 7, borderRadius: 3.5, marginHorizontal: 2.5 },
  spinner: { width: 12, height: 12, transform: [{ scale: 0.6 }] },
  rowText: { flex: 1 },
  title: { color: theme.text, fontSize: 15 },
  cwd: { color: theme.muted, fontSize: 11, marginTop: 2 },
  label: { fontSize: 12, fontWeight: "600" },
});
