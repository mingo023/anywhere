import React, { useState } from "react";
import { StyleSheet, View } from "react-native";
import { StatusBar } from "expo-status-bar";
import { useFonts } from "expo-font";
import { Geist_400Regular, Geist_500Medium, Geist_600SemiBold } from "@expo-google-fonts/geist";
import { GeistMono_400Regular, GeistMono_500Medium, GeistMono_600SemiBold } from "@expo-google-fonts/geist-mono";
import { SafeAreaProvider, SafeAreaView } from "react-native-safe-area-context";
import { d } from "./design";
import { SessionProvider, useSession } from "./session";
import { ConnectScreen } from "./screens/ConnectScreen";
import { AgentsScreen } from "./screens/AgentsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { PermissionSheet } from "./components/PermissionSheet";

function Root() {
  const { state, permission, resolvePermission } = useSession();
  const [agentId, setAgentId] = useState<string>();

  return (
    <>
      {state === "online" && agentId ? (
        <ChatScreen agentId={agentId} onBack={() => setAgentId(undefined)} />
      ) : (
        <SafeAreaView style={styles.root} edges={["top", "bottom"]}>
          {state === "online" ? <AgentsScreen onOpen={setAgentId} /> : <ConnectScreen />}
        </SafeAreaView>
      )}
      <PermissionSheet request={permission} onResolve={resolvePermission} />
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
