import React from "react";
import type { StyleProp, ViewStyle } from "react-native";
import { BlurView } from "expo-blur";
import { GlassView, isGlassEffectAPIAvailable, isLiquidGlassAvailable } from "expo-glass-effect";

/** Some iOS 26 betas ship the OS without the API and crash on GlassView, so both checks are needed. */
const supported = isLiquidGlassAvailable() && isGlassEffectAPIAvailable();

type Props = {
  style?: StyleProp<ViewStyle>;
  interactive?: boolean;
  onLayout?: React.ComponentProps<typeof BlurView>["onLayout"];
  children: React.ReactNode;
};

export function Glass({ style, interactive = false, onLayout, children }: Props) {
  if (supported) {
    return (
      <GlassView style={style} glassEffectStyle="regular" colorScheme="dark" isInteractive={interactive} onLayout={onLayout}>
        {children}
      </GlassView>
    );
  }

  return (
    <BlurView style={style} intensity={40} tint="dark" onLayout={onLayout}>
      {children}
    </BlurView>
  );
}
