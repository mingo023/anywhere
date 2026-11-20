import React from "react";
import { AbsoluteFill, Easing, Img, interpolate, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import { Audio } from "@remotion/media";
import { D, Dot, FPB, GRADIENT, SYS, TL, clamp, outExpo, outQuint, press, ramp, track } from "./kit";
import { Hosts } from "./Hosts";
import { Main } from "./Main";
import { HostIcon, MacIcon, Sidebar } from "./Sidebar";

export const TOTAL = Math.round(TL.beats * FPB);
const SWITCH = 22.6;
const BACK = 46.05;
// 220ms, the app's SLIDE, in beats.
const SLIDE = (0.22 * TL.bpm) / 60;

/** The whole app window, 1440×900 points, at beat `b`. */
export const AppWindow: React.FC<{ b: number; ms: number; pin?: boolean }> = ({ b, ms, pin }) => {
  const main = ramp(b, SWITCH, 0.3);
  const back = !pin && b >= BACK;
  const slide = back ? ramp(b, BACK, SLIDE, outQuint) : ramp(b, SWITCH, SLIDE, outQuint);
  return (
    <div style={{ position: "absolute", width: 1440, height: 900, fontFamily: SYS, color: D.text, WebkitFontSmoothing: "antialiased", overflow: "hidden", borderRadius: 14 }}>
      {main < 1 && (
        <div style={{ position: "absolute", inset: 0, opacity: 1 - main }}>
          <Hosts b={b} ms={ms} />
        </div>
      )}
      {main > 0 && (
        <div style={{ position: "absolute", inset: 0, opacity: main }}>
          <Sidebar
            space={back ? "mac" : "gpu"}
            slide={slide}
            gpuHost
            project={ramp(b, 28.2, 0.4)}
            session={ramp(b, 34.3, 0.4)}
            ms={ms}
            pressedIcon={press(b, [46])}
          />
          <Main b={b} ms={ms} mac={back ? ramp(b, BACK, 0.3) : 0} />
        </div>
      )}
    </div>
  );
};

const FULL = [720, 450, 1.15];

const Glass: React.FC<{ fx: number; fy: number; z: number; children: React.ReactNode }> = ({ fx, fy, z, children }) => (
  <div
    style={{
      position: "absolute",
      left: 0,
      top: 0,
      width: 1440,
      height: 900,
      transformOrigin: "0 0",
      transform: `translate(${960 - fx * z}px, ${540 - fy * z}px) scale(${z})`,
      borderRadius: 14,
      background: D.window,
      backdropFilter: "blur(50px) saturate(1.6)",
      boxShadow: "0 0 0 1px rgba(255,255,255,0.10), 0 40px 120px rgba(0,0,0,0.65)",
    }}
  >
    {children}
  </div>
);
const CAMERA = [
  [5, ...FULL],
  [7.6, ...FULL],
  [8.6, 856, 378, 2.05],
  [11.6, 856, 378, 2.05],
  [12.4, 876, 400, 2.1],
  [22.4, 876, 400, 2.1],
  [23.6, ...FULL],
  [25.05, ...FULL],
  [25.8, 720, 450, 1.32],
  [28.1, 720, 450, 1.32],
  [29.2, 833, 470, 1.9],
  [34.1, 833, 470, 1.9],
  [34.9, 833, 340, 1.6],
  [42.9, 833, 340, 1.6],
  [44.2, ...FULL],
  [46.1, ...FULL],
  [47.2, 430, 430, 1.75],
  [48.6, 430, 430, 1.75],
  [49.6, ...FULL],
];

const POINTER = [
  [6, 1000, 640],
  [7.3, 1116, 289],
  [7.6, 1116, 289],
  [8.4, 700, 338],
  [8.8, 700, 338],
  [9.4, 770, 380],
  [10.8, 1125, 463],
  [11.6, 1125, 463],
  [12.4, 1070, 490],
  [21.6, 1070, 490],
  [22.2, 1046, 336],
  [22.6, 1046, 336],
  [24.5, 95, 376],
  [25.1, 95, 376],
  [25.8, 470, 209],
  [26.1, 470, 209],
  [26.7, 690, 209],
  [27.1, 690, 209],
  [27.7, 1141, 731],
  [28.1, 1141, 731],
  [28.9, 760, 462],
  [29.4, 760, 462],
  [30.1, 930, 525],
  [33.3, 930, 525],
  [33.9, 1103, 513],
  [34.2, 1103, 513],
  [35, 1180, 640],
  [44.4, 300, 700],
  [45.7, 25, 872],
  [47.5, 25, 872],
];

const pointerOpacity = (b: number) =>
  Math.min(ramp(b, 6, 0.3), 1 - ramp(b, 34.8, 0.4)) + Math.min(ramp(b, 44.4, 0.3), 1 - ramp(b, 47.2, 0.4));

const Pointer: React.FC<{ x: number; y: number; opacity: number; pressed: number; ring: number }> = ({ x, y, opacity, pressed, ring }) => (
  <>
    {ring > 0 && ring < 1 && (
      <div
        style={{
          position: "absolute",
          left: x - 40,
          top: y - 40,
          width: 80,
          height: 80,
          borderRadius: 40,
          border: "2px solid rgba(235,235,235,0.7)",
          background: "rgba(235,235,235,0.12)",
          scale: 0.25 + ring * 0.9,
          opacity: (1 - ring) * opacity,
        }}
      />
    )}
    <svg
      width={40}
      height={40}
      viewBox="0 0 24 24"
      style={{ position: "absolute", left: x - 7.6, top: y - 4, opacity, scale: 1 - pressed * 0.2, transformOrigin: "19% 10%", filter: "drop-shadow(0 4px 10px rgba(0,0,0,0.5))" }}
    >
      <path d="M4.5 2.5 L4.5 19.5 L9 15.2 L12 21.5 L14.8 20.3 L11.9 14.2 L18 14.2 Z" fill="#111" stroke="#fff" strokeWidth={1.5} strokeLinejoin="round" />
    </svg>
  </>
);

const Wallpaper: React.FC<{ b: number }> = ({ b }) => (
  <AbsoluteFill style={{ background: "#0a0a0c", overflow: "hidden" }}>
    {[
      { c: "rgba(62,111,216,0.55)", x: 260, y: 180, r: 1100, s: 0 },
      { c: "rgba(124,79,208,0.45)", x: 1250, y: 420, r: 1200, s: 2 },
      { c: "rgba(217,120,90,0.35)", x: 1600, y: 980, r: 1000, s: 4 },
    ].map((g) => (
      <div
        key={g.c}
        style={{
          position: "absolute",
          left: g.x - g.r / 2 + Math.sin(b / 9 + g.s) * 70,
          top: g.y - g.r / 2 + Math.cos(b / 11 + g.s) * 50,
          width: g.r,
          height: g.r,
          borderRadius: "50%",
          background: `radial-gradient(closest-side, ${g.c}, rgba(0,0,0,0))`,
        }}
      />
    ))}
  </AbsoluteFill>
);

const Gradient: React.FC<{ children: React.ReactNode; wipe: number }> = ({ children, wipe }) => (
  <span style={{ position: "relative", display: "inline-block" }}>
    {children}
    <span
      style={{
        position: "absolute",
        inset: 0,
        backgroundImage: `linear-gradient(90deg, ${GRADIENT.join(", ")})`,
        WebkitBackgroundClip: "text",
        WebkitTextFillColor: "transparent",
        clipPath: `inset(0 ${(1 - wipe) * 100}% 0 0)`,
      }}
    >
      {children}
    </span>
  </span>
);

/** Words that blur in one after another from beat `at`. */
const Words: React.FC<{ b: number; at: number; text: string; size: number; color?: string; weight?: number; gradientFrom?: number; wipeAt?: number }> = ({
  b,
  at,
  text,
  size,
  color = D.text,
  weight = 600,
  gradientFrom,
  wipeAt = at + 1,
}) => {
  const words = text.split(" ");
  const wipe = ramp(b, wipeAt, 0.6);
  return (
    <div style={{ fontFamily: SYS, fontSize: size, fontWeight: weight, letterSpacing: "-0.03em", color, whiteSpace: "nowrap", lineHeight: 1.12 }}>
      {words.map((w, i) => {
        const p = ramp(b, at + i * 0.18, 0.7);
        const g = gradientFrom !== undefined && i >= gradientFrom;
        return (
          <span key={i} style={{ display: "inline-block", opacity: p, translate: `0 ${(1 - p) * size * 0.3}px`, filter: `blur(${(1 - p) * 12}px)`, marginRight: "0.25em" }}>
            {g ? <Gradient wipe={wipe}>{w}</Gradient> : w}
          </span>
        );
      })}
    </div>
  );
};

const Hook: React.FC<{ b: number }> = ({ b }) => {
  const out = ramp(b, 4.2, 0.8, Easing.in(Easing.cubic));
  return (
    <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", opacity: 1 - out, scale: 1 + out * 0.25, filter: `blur(${out * 20}px)` }}>
      <div style={{ textAlign: "center" }}>
        <Words b={b} at={0.2} text="Your agents," size={112} />
        <Words b={b} at={1.2} text="on every machine." size={112} gradientFrom={1} wipeAt={2.2} />
      </div>
    </AbsoluteFill>
  );
};

const PAIR = { at: 48.6, scale: 0.5, dx: 440, dy: 80 };

const Tagline: React.FC<{ b: number }> = ({ b }) => {
  const out = ramp(b, 53, 0.6, Easing.in(Easing.cubic));
  const label = (x: number, at: number, icon: React.ReactNode, name: string) => {
    const p = ramp(b, at, 0.6);
    return (
      <div style={{ position: "absolute", left: x - 150, width: 300, top: 540 + PAIR.dy + (1035 * PAIR.scale) / 2 + 22, display: "flex", justifyContent: "center", alignItems: "center", gap: 12, fontFamily: SYS, fontSize: 28, fontWeight: 600, color: D.text, opacity: p * (1 - out), translate: `0 ${(1 - p) * 16}px` }}>
        {icon}
        {name}
        <Dot color={D.success} size={9} />
      </div>
    );
  };
  return (
    <>
      <div style={{ position: "absolute", left: 0, right: 0, top: 150, display: "flex", justifyContent: "center", opacity: 1 - out, filter: `blur(${out * 14}px)` }}>
        <Words b={b} at={49.4} text="Your Mac and gpu-box, working at once." size={64} gradientFrom={4} wipeAt={50.6} />
      </div>
      {label(960 - PAIR.dx, 49.6, <MacIcon size={30} />, "This Mac")}
      {label(960 + PAIR.dx, 50, <HostIcon color={D.pink} size={30} />, "gpu-box")}
    </>
  );
};

// Eye centres in the icon, from scripts/blink-assets.py.
const EYES = [
  { src: "AppIcon-eye-l.png", origin: "44.97% 45.36%" },
  { src: "AppIcon-eye-r.png", origin: "58.89% 48.34%" },
];
const BLINK = TL.blink;

/** The app icon, its eyes on their own layers so it can blink once on the beat. */
const Mascot: React.FC<{ b: number; style: React.CSSProperties }> = ({ b, style }) => {
  const shut = interpolate(b, [BLINK - 0.14, BLINK, BLINK + 0.07, BLINK + 0.3], [0, 1, 1, 0], { ...clamp, easing: Easing.inOut(Easing.quad) });
  const layer: React.CSSProperties = { position: "absolute", inset: 0, width: 190, height: 190 };
  return (
    <div style={{ position: "relative", width: 190, height: 190, ...style }}>
      <div style={{ ...layer, scale: `${1 + shut * 0.015} ${1 - shut * 0.03}`, transformOrigin: "50% 90%" }}>
        <Img src={staticFile("AppIcon-face.png")} style={layer} />
        {EYES.map((e) => (
          <Img key={e.src} src={staticFile(e.src)} style={{ ...layer, transformOrigin: e.origin, scale: `1 ${1 - shut * 0.9}` }} />
        ))}
      </div>
    </div>
  );
};

const End: React.FC<{ b: number }> = ({ b }) => {
  const icon = ramp(b, 54.2, 0.9);
  const fade = ramp(b, 66, 2, Easing.inOut(Easing.cubic));
  return (
    <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", flexDirection: "column", opacity: 1 - fade }}>
      <Mascot b={b} style={{ opacity: icon, scale: 0.6 + 0.4 * icon, filter: `blur(${(1 - icon) * 16}px)` }} />
      <div style={{ height: 26 }} />
      <Words b={b} at={54.7} text="Anywhere" size={84} />
      <div style={{ height: 14 }} />
      <Words b={b} at={55.5} text="Remote hosts" size={44} weight={600} gradientFrom={0} wipeAt={55.7} />
      <div style={{ height: 10 }} />
      <Words b={b} at={56.4} text="Run agents on any machine you can reach over SSH." size={30} weight={500} color={D.t2} />
      <div style={{ height: 44 }} />
      <div style={{ display: "flex", gap: 16 }}>
        {[
          { name: "This Mac", icon: <MacIcon size={34} /> },
          { name: "devbox", icon: <HostIcon color={D.blue} size={34} /> },
          { name: "gpu-box", icon: <HostIcon color={D.pink} size={34} /> },
        ].map((h, i) => {
          const p = ramp(b, 58 + i * 0.5, 0.6);
          return (
            <div
              key={h.name}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 14,
                padding: "12px 22px 12px 14px",
                borderRadius: 18,
                background: "rgba(235,235,235,0.08)",
                border: `1px solid ${D.hairline}`,
                fontFamily: SYS,
                fontSize: 26,
                fontWeight: 600,
                color: D.text,
                opacity: p,
                translate: `0 ${(1 - p) * 30}px`,
                filter: `blur(${(1 - p) * 8}px)`,
              }}
            >
              {h.icon}
              {h.name}
              <Dot color={D.success} size={10} />
            </div>
          );
        })}
      </div>
    </AbsoluteFill>
  );
};

export const Video: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const b = frame / FPB;
  const ms = (frame / fps) * 1000;
  const [fx, fy, z] = track(b, CAMERA);
  const [px, py] = track(b, POINTER, Easing.bezier(0.45, 0, 0.15, 1));
  const intro = ramp(b, TL.drop, 1.6, outExpo);
  const pair = ramp(b, PAIR.at, 1.4, Easing.bezier(0.33, 0, 0.15, 1));
  const enter = ramp(b, PAIR.at + 0.4, 1.2, outExpo);
  const exit = ramp(b, 53.2, 0.8, Easing.in(Easing.cubic));
  const clicks = TL.clicks.filter((c) => b >= c && b < c + 1);
  const ring = clicks.length ? interpolate(b - clicks[clicks.length - 1], [0, 0.9], [0, 1], { ...clamp, easing: Easing.out(Easing.cubic) }) : 0;
  return (
    <AbsoluteFill style={{ background: "#0a0a0c" }}>
      <Wallpaper b={b} />
      {b < 5.2 && <Hook b={b} />}
      {b >= TL.drop && b < 54.2 && (
        <AbsoluteFill style={{ perspective: 2600 }}>
          <AbsoluteFill
            style={{
              opacity: interpolate(intro, [0, 0.25], [0, 1], clamp) * (1 - exit),
              filter: exit > 0 ? `blur(${exit * 18}px)` : undefined,
              transform: [
                `translate(${-PAIR.dx * pair - 300 * exit}px, ${(1 - intro) * 220 + PAIR.dy * pair}px)`,
                `rotateX(${(1 - intro) * 24}deg)`,
                `rotateY(${(1 - intro) * -14 + pair * 8}deg)`,
                `scale(${(0.82 + 0.18 * intro) * (1 - (1 - PAIR.scale) * pair)})`,
              ].join(" "),
            }}
          >
            <Glass fx={fx} fy={fy} z={z}>
              <AppWindow b={b} ms={ms} />
            </Glass>
            <Pointer x={960 + (px - fx) * z} y={540 + (py - fy) * z} opacity={pointerOpacity(b)} pressed={press(b, TL.clicks)} ring={ring} />
          </AbsoluteFill>
          {b >= PAIR.at && (
            <AbsoluteFill
              style={{
                opacity: Math.min(1, enter * 2) * (1 - exit),
                filter: exit > 0 ? `blur(${exit * 18}px)` : undefined,
                transform: `translate(${PAIR.dx + (1 - enter) * 900 + 300 * exit}px, ${PAIR.dy}px) rotateY(${-8}deg) scale(${PAIR.scale})`,
              }}
            >
              <Glass fx={FULL[0]} fy={FULL[1]} z={FULL[2]}>
                <AppWindow b={b} ms={ms} pin />
              </Glass>
            </AbsoluteFill>
          )}
        </AbsoluteFill>
      )}
      {b >= 48.6 && b < 54.2 && <Tagline b={b} />}
      {b >= 54 && <End b={b} />}
      <Audio src={staticFile("music.wav")} volume={0.9} />
      <Audio src={staticFile("sfx.wav")} volume={1} />
    </AbsoluteFill>
  );
};
