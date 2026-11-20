import React from "react";
import { Easing, interpolate } from "remotion";
import { loadFont } from "@remotion/google-fonts/GeistMono";
import tl from "../timeline.json";
import { ICONS } from "./icons.gen";

export const TL = tl;
export const FPB = (60 * tl.fps) / tl.bpm;
export const SYS = '-apple-system, "SF Pro Text", "Helvetica Neue", sans-serif';
export const { fontFamily: MONO } = loadFont("normal", { weights: ["400", "500"], subsets: ["latin"] });

// Graphite Dark, from `Token::new(light, dark)` in crates/theme/src/theme.rs.
export const D = {
  text: "#ebebeb",
  body: "#bcbcbc",
  t2: "#a1a1a1",
  t3: "#818181",
  t4: "#6c6c6c",
  t5: "#616161",
  t6: "#414141",
  onText: "#171717",
  window: "rgba(18,18,18,0.85)",
  surface: "rgba(235,235,235,0.10)",
  fill1: "rgba(235,235,235,0.05)",
  fill2: "rgba(235,235,235,0.08)",
  fill3: "rgba(235,235,235,0.10)",
  fill4: "rgba(235,235,235,0.12)",
  hairline: "rgba(235,235,235,0.07)",
  separator: "rgba(235,235,235,0.08)",
  sepStrong: "rgba(235,235,235,0.14)",
  popover: "#232323",
  glass: "rgba(42,42,42,0.8)",
  bubble: "#2c2c30",
  waiting: "#ffb224",
  waitingBg: "rgba(255,178,36,0.18)",
  failed: "#e5484d",
  failedText: "#ff9592",
  success: "#30a46c",
  successText: "#3dd68c",
  claude: "#d97757",
  blue: "#3e8ef7",
  pink: "#d6409f",
  synKw: "#d19dff",
  synFn: "#9eb1ff",
  synStr: "#3dd68c",
};

export const GRADIENT = ["#8fb8ff", "#a9aafa", "#c39cf5", "#e19db7", "#ff9f7a"];

export const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
export const outExpo = Easing.bezier(0.16, 1, 0.3, 1);
export const outQuint = Easing.bezier(0.22, 1, 0.36, 1);
export const inOut = Easing.bezier(0.65, 0, 0.35, 1);

/** 0→1 from beat `from` over `dur` beats. */
export const ramp = (b: number, from: number, dur = 0.5, easing = outExpo) => interpolate(b, [from, from + dur], [0, 1], { ...clamp, easing });

/** Piecewise `[beat, ...values]` keys, eased in and out between each. */
export const track = (b: number, ks: number[][], easing = inOut) =>
  ks[0].slice(1).map((_, j) => interpolate(b, ks.map((k) => k[0]), ks.map((k) => k[j + 1]), { ...clamp, easing }));

/** The beat each character of a `typing` entry lands on; the audio script uses the same sums. */
export const charBeats = (at: number, text: string, cps: number) => {
  const out: number[] = [];
  let s = (at * 60) / tl.bpm;
  for (let i = 0; i < text.length; i++) {
    out.push((s * tl.bpm) / 60);
    s += tl.jitter[i % tl.jitter.length] / cps;
  }
  return out;
};

export const typed = (b: number, i: number) => {
  const t = tl.typing[i];
  return t.text.slice(0, charBeats(t.at, t.text, t.cps).filter((c) => c <= b).length);
};

export const Icon: React.FC<{ name: string; size: number; color: string; style?: React.CSSProperties }> = ({ name, size, color, style }) => (
  <span
    style={{ width: size, height: size, color, display: "inline-flex", flex: "none", ...style }}
    dangerouslySetInnerHTML={{ __html: ICONS[name] }}
  />
);

/** The app's `spinner`: the 8-spoke icon turned a whole spoke at a time, period 800ms, 10fps. */
export const Spinner: React.FC<{ ms: number; size: number; color: string }> = ({ ms, size, color }) => {
  const t = ((Math.floor(ms / 100) * 100) % 800) / 800;
  return <Icon name="spinner" size={size} color={color} style={{ rotate: `${(Math.round(t * 8) % 8) * 45}deg` }} />;
};

const gradientOpacity = (t: number) => {
  t = ((t % 1) + 1) % 1;
  if (t < 0.45) return 1 + (0.1 - 1) * (t / 0.45);
  if (t < 0.92) return 0.1;
  return 0.1 + 0.9 * ((t - 0.92) / 0.08);
};

/** The app's `gradient_spinner`: a 3×3 grid of dots, a pulse sweeping from the top-left, period 800ms, 20fps. */
export const GradientSpinner: React.FC<{ ms: number; size: number }> = ({ ms, size }) => {
  const t = ((Math.floor(ms / 50) * 50) % 800) / 800;
  const dot = size / 4;
  return (
    <div style={{ width: size, height: size, display: "grid", gridTemplateColumns: `repeat(3, ${dot}px)`, gap: dot / 2, flex: "none", alignContent: "center", justifyContent: "center" }}>
      {[0, 1, 2].flatMap((r) =>
        [0, 1, 2].map((c) => (
          <div key={`${r}${c}`} style={{ width: dot, height: dot, borderRadius: dot, background: GRADIENT[r + c], opacity: gradientOpacity(t - (r + c) / 8) }} />
        )),
      )}
    </div>
  );
};

const mix = (a: string, b: string, k: number) => {
  const p = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [x, y] = [p(a), p(b)];
  return `rgb(${x.map((v, i) => Math.round(v + (y[i] - v) * k)).join(",")})`;
};

/** The app's `sweep`: a band of light 0.6 labels wide crossing a working label every 2800ms, 20fps. */
export const Sweep: React.FC<{ text: string; ms: number; base: string }> = ({ text, ms, base }) => {
  const t = ((Math.floor(ms / 50) * 50) % 2800) / 2800;
  const n = Math.max(1, text.length);
  const centre = -0.3 + 1.6 * t;
  return (
    <span>
      {[...text].map((c, i) => (
        <span key={i} style={{ color: mix(base, "#ffffff", Math.max(0, 1 - Math.abs((i + 0.5) / n - centre) / 0.3)) }}>
          {c}
        </span>
      ))}
    </span>
  );
};

export const Btn: React.FC<{ children: React.ReactNode; kind?: "gray" | "red" | "white"; pressed?: number; style?: React.CSSProperties }> = ({
  children,
  kind = "gray",
  pressed = 0,
  style,
}) => {
  const k: Record<string, React.CSSProperties> = {
    gray: { background: "rgba(235,235,235,0.16)", color: D.text, borderRadius: 7 },
    red: { background: D.failed, color: "#fff", borderRadius: 13, fontWeight: 600 },
    white: { background: D.text, color: D.onText, borderRadius: 13, fontWeight: 600 },
  };
  return (
    <div
      style={{
        height: 26,
        padding: "0 11px",
        display: "flex",
        alignItems: "center",
        gap: 5,
        fontSize: 13,
        whiteSpace: "nowrap",
        scale: 1 - pressed * 0.06,
        filter: `brightness(${1 - pressed * 0.15})`,
        ...k[kind],
        ...style,
      }}
    >
      {children}
    </div>
  );
};

export const Dot: React.FC<{ color: string; size?: number; style?: React.CSSProperties }> = ({ color, size = 6, style }) => (
  <span style={{ display: "inline-block", width: size, height: size, borderRadius: size, background: color, flex: "none", ...style }} />
);

export const Caret: React.FC<{ b: number; h?: number }> = ({ b, h = 16 }) => (
  <span style={{ display: "inline-block", width: 1.5, height: h, background: D.text, marginLeft: 1, verticalAlign: "middle", opacity: Math.floor(b * 2) % 2 === 0 ? 1 : 0.15 }} />
);

/** How far a click at beat `c` presses its target, 0–1. */
export const press = (b: number, clicks: number[]) => clicks.reduce((s, c) => Math.max(s, interpolate(b, [c - 0.15, c, c + 0.3], [0, 1, 0], clamp)), 0);
