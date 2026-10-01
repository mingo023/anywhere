import React, { useCallback, useEffect, useState } from "react";
import { Linking, StyleSheet, View } from "react-native";
import { StatusBar } from "expo-status-bar";
import { useFonts } from "expo-font";
import { Geist_400Regular, Geist_500Medium, Geist_600SemiBold } from "@expo-google-fonts/geist";
import { GeistMono_400Regular, GeistMono_500Medium, GeistMono_600SemiBold } from "@expo-google-fonts/geist-mono";
import { SafeAreaProvider, SafeAreaView } from "react-native-safe-area-context";
import { d } from "./design";
import { SessionProvider, useSession } from "./session";
import { parsePairURL, type PairLink } from "./pairing";
import { AgentsScreen } from "./screens/AgentsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { NewSessionScreen } from "./screens/NewSessionScreen";
import { ConfirmPairScreen } from "./screens/ConfirmPairScreen";
import { EnterCodeScreen } from "./screens/EnterCodeScreen";
import { KeychainScreen } from "./screens/KeychainScreen";
import { PairScreen } from "./screens/PairScreen";
import { ReconnectScreen } from "./screens/ReconnectScreen";
import { RevokedScreen } from "./screens/RevokedScreen";
import { UpdateScreen } from "./screens/UpdateScreen";
import { PermissionSheet } from "./components/PermissionSheet";
import { header, pending } from "./permissions";

function usePairLink(): [PairLink | undefined, (link: PairLink | undefined) => void] {
  const [link, setLink] = useState<PairLink>();
  useEffect(() => {
    const take = (url: string | null) => {
      const parsed = url ? parsePairURL(url) : null;
      if (parsed) setLink(parsed);
    };
    Linking.getInitialURL().then(take, () => {});
    const sub = Linking.addEventListener("url", (event) => take(event.url));
    return () => sub.remove();
  }, []);
  return [link, setLink];
}

function Screen({ signedIn, onOpen, onLink, onNew }: { signedIn: boolean; onOpen: (agentId: string) => void; onLink: (link: PairLink) => void; onNew: () => void }) {
  const { creds, unreadable, ended, retry, pairAgain } = useSession();
  const [typing, setTyping] = useState(false);

  if (unreadable) return <KeychainScreen onRetry={retry} />;
  if (ended === "revoked") return <RevokedScreen onPairAgain={pairAgain} />;
  if (ended) return <UpdateScreen side={ended} onRetry={retry} />;
  if (signedIn) return <AgentsScreen onOpen={onOpen} onNew={onNew} />;
  if (creds) return <ReconnectScreen creds={creds} />;
  if (typing) return <EnterCodeScreen onContinue={onLink} onBack={() => setTyping(false)} />;
  return <PairScreen onEnterCode={() => setTyping(true)} />;
}

function Root() {
  const { link: connection, creds, unreadable, agents, permissions, resolvePermission } = useSession();
  const [agentId, setAgentId] = useState<string>();
  const [link, setLink] = usePairLink();
  const [creating, setCreating] = useState(false);
  const created = useCallback((id: string) => {
    setCreating(false);
    setAgentId(id);
  }, []);
  const open = agents.some((a) => a.id === agentId) ? agentId : undefined;
  const signedIn = connection.state !== "idle" && connection.state !== "rejected";

  if (creds === undefined && !unreadable) return null;
  return (
    <>
      {signedIn && open && !link ? (
        <ChatScreen agentId={open} onBack={() => setAgentId(undefined)} />
      ) : (
        <SafeAreaView style={styles.root} edges={["top", "bottom"]}>
          {link ? (
            <ConfirmPairScreen key={link.code} link={link} onDone={() => setLink(undefined)} />
          ) : signedIn && creating ? (
            <NewSessionScreen onClose={() => setCreating(false)} onCreated={created} />
          ) : (
            <Screen signedIn={signedIn} onOpen={setAgentId} onLink={setLink} onNew={() => setCreating(true)} />
          )}
        </SafeAreaView>
      )}
      <PermissionSheet request={pending(permissions)[0]} header={header(permissions)} onResolve={resolvePermission} />
    </>
  );
}

export default function App() {
  const [fontsLoaded] = useFonts({
    Geist_400Regular,
    Geist_500Medium,
    Geist_600SemiBold,
    GeistMono_400Regular,
    GeistMono_500Medium,
    GeistMono_600SemiBold,
  });

  return (
    <SafeAreaProvider>
      <View style={styles.root}>
        <StatusBar style="light" />
        {fontsLoaded ? (
          <SessionProvider>
            <Root />
          </SessionProvider>
        ) : null}
      </View>
    </SafeAreaProvider>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: d.bg },
});
