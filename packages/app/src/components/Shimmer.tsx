import React, { useEffect, useRef, useState } from "react";
import { Animated, Easing, StyleSheet, Text, View, type TextStyle } from "react-native";
import MaskedView from "@react-native-masked-view/masked-view";
import { LinearGradient } from "expo-linear-gradient";

type Props = {
  children: string;
  style: TextStyle;
  base: string;
  highlight: string;
};

export function Shimmer({ children, style, base, highlight }: Props) {
  const [box, setBox] = useState({ width: 0, height: 0 });
  const progress = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    if (!box.width) return;
    progress.setValue(0);
    const loop = Animated.loop(
      Animated.timing(progress, { toValue: 1, duration: 1600, easing: Easing.linear, useNativeDriver: true }),
    );
    loop.start();
    return () => loop.stop();
  }, [box.width, progress]);

  const translateX = progress.interpolate({
    inputRange: [0, 1],
    outputRange: [-box.width, box.width],
  });

  return (
    <View>
      <Text style={[style, styles.measure]} onLayout={(event) => setBox(event.nativeEvent.layout)}>
        {children}
      </Text>
      {box.width ? (
        <MaskedView style={StyleSheet.absoluteFill} maskElement={<Text style={style}>{children}</Text>}>
          <View style={[StyleSheet.absoluteFill, { backgroundColor: base }]} />
          <Animated.View style={[StyleSheet.absoluteFill, { transform: [{ translateX }] }]}>
            <LinearGradient
              colors={[`${base}00`, highlight, `${base}00`]}
              start={{ x: 0, y: 0 }}
              end={{ x: 1, y: 0 }}
              style={StyleSheet.absoluteFill}
            />
          </Animated.View>
        </MaskedView>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  measure: { opacity: 0 },
});
