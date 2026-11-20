import React from "react";
import { interpolate } from "remotion";
import { Btn, Caret, D, Dot, Icon, MONO, Spinner, TL, clamp, press, ramp, typed } from "./kit";
import { HostIcon, MacIcon, SIDE_W } from "./Sidebar";

export const CARD = { x: 536, y: 164, w: 640 };
const STEPS = ["Check the host", "Download pocketd", "Verify the download", "Unpack pocketd", "Install pocketd", "Set up the service", "Start pocketd"];
export const CUE = { add: 7.5, field: 8.75, connect: 11.5, firstStep: 11.75, open: 22.5 };

const SETTINGS: [string, string, number][] = [
  ["App", "", 128],
  ["sliders", "General", 155],
  ["appearance", "Appearance", 186],
  ["bell", "Notifications & Sounds", 217],
  ["keyboard", "Keyboard", 248],
  ["Agents", "", 283],
  ["sparkle", "Agents", 310],
  ["shield", "Accounts", 341],
  ["clock", "Automations", 372],
  ["phone", "Phone & Remote", 403],
  ["server", "Hosts", 433],
  ["Projects", "", 469],
  ["folder", "Projects", 496],
  ["sidebar", "Sidebar & Inbox", 527],
  ["terminal", "Terminal", 558],
  ["file", "Files & Editor", 588],
  ["globe", "Browser", 619],
  ["Git", "", 655],
  ["branch", "Git & GitHub", 682],
  ["diff-split", "Diff & Graph", 713],
];

const SettingsSide: React.FC = () => (
  <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: SIDE_W, borderRight: `1px solid ${D.separator}` }}>
    <div style={{ position: "absolute", left: 14, top: 14, display: "flex", gap: 8 }}>
      {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
        <Dot key={c} color={c} size={12} />
      ))}
    </div>
    <div style={{ position: "absolute", left: 20, top: 47, display: "flex", alignItems: "center", gap: 9, fontSize: 13, color: D.body }}>
      <Icon name="back" size={14} color={D.t2} />
      Back
    </div>
    <div style={{ position: "absolute", left: 12, right: 11, top: 80, height: 30, borderRadius: 8, background: D.fill2, display: "flex", alignItems: "center", padding: "0 10px", gap: 10, fontSize: 13, color: D.t3 }}>
      <Icon name="search" size={14} color={D.t3} />
      <span style={{ flex: 1 }}>Search</span>
      <span style={{ fontSize: 12, color: D.t4 }}>⌘F</span>
    </div>
    {SETTINGS.map(([icon, label, y]) =>
      label === "" ? (
        <div key={icon} style={{ position: "absolute", left: 18, top: y - 8, fontSize: 11, fontWeight: 600, color: D.t3 }}>
          {icon}
        </div>
      ) : (
        <div
          key={label}
          style={{
            position: "absolute",
            left: 10,
            right: 10,
            top: y - 15,
            height: 30,
            borderRadius: 7,
            background: label === "Hosts" ? D.fill4 : undefined,
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "0 10px",
            fontSize: 13,
            color: label === "Hosts" ? D.text : D.body,
          }}
        >
          <Icon name={icon} size={16} color={label === "Hosts" ? D.text : D.t2} />
          {label}
        </div>
      ),
    )}
  </div>
);

const Row: React.FC<{ icon: React.ReactNode; title: string; line: React.ReactNode; right: React.ReactNode; children?: React.ReactNode }> = ({ icon, title, line, right, children }) => (
  <div style={{ borderTop: `1px solid ${D.separator}`, padding: "0 14px 0 14.5px" }}>
    <div style={{ height: 51, display: "flex", alignItems: "center", gap: 12.5 }}>
      {icon}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, color: D.text, lineHeight: "16px" }}>{title}</div>
        <div style={{ fontSize: 12, color: D.t3, lineHeight: "15px", display: "flex", alignItems: "center", gap: 6 }}>{line}</div>
      </div>
      <div style={{ display: "flex", gap: 9 }}>{right}</div>
    </div>
    {children}
  </div>
);

const Check: React.FC<{ p: number }> = ({ p }) => (
  <svg width={12} height={12} viewBox="0 0 12 12">
    <path d="M2 6.4 L4.8 9 L10 3" fill="none" stroke={D.success} strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round" strokeDasharray={12} strokeDashoffset={12 * (1 - p)} />
  </svg>
);

const Checklist: React.FC<{ b: number; ms: number }> = ({ b, ms }) => {
  const ready = ramp(b, TL.ready, 0.4);
  const open = press(b, [CUE.open]);
  return (
    <div style={{ margin: "0 0 10px 40.5px", background: D.fill2, borderRadius: 8, padding: "8px 10px 6px" }}>
      <div style={{ height: 22, display: "flex", alignItems: "center", justifyContent: "space-between", fontSize: 13, color: D.text }}>
        <span style={{ fontWeight: ready > 0.5 ? 400 : 500 }}>{ready > 0.5 ? "gpu-box is ready." : "Installing pocketd"}</span>
        <div style={{ display: "flex", gap: 6, opacity: ready, scale: 0.9 + ready * 0.1 }}>
          <Btn kind="white" pressed={open}>Open gpu-box</Btn>
          <Btn style={{ borderRadius: 13 }}>Done</Btn>
        </div>
      </div>
      {STEPS.map((s, i) => {
        const start = i === 0 ? CUE.firstStep : TL.steps[i - 1];
        const done = ramp(b, TL.steps[i], 0.3);
        const active = b >= start && b < TL.steps[i];
        return (
          <div key={s} style={{ height: 22, display: "flex", alignItems: "center", gap: 8, fontSize: 13 }}>
            <div style={{ width: 13, display: "grid", placeItems: "center" }}>
              {b >= TL.steps[i] ? <Check p={done} /> : active ? <Spinner ms={ms} size={13} color={D.t3} /> : <Dot color={D.t5} size={4} />}
            </div>
            <span style={{ color: b >= start ? D.text : D.t4 }}>{s}</span>
          </div>
        );
      })}
    </div>
  );
};

const Field: React.FC<{ x: number; y: number; w: number; value: string; faint?: boolean; placeholder?: string; caret?: React.ReactNode; focus?: number }> = ({
  x,
  y,
  w,
  value,
  faint,
  placeholder,
  caret,
  focus = 0,
}) => (
  <div
    style={{
      position: "absolute",
      left: x,
      top: y,
      width: w,
      height: 26,
      borderRadius: 6,
      background: D.fill2,
      boxShadow: `0 0 0 ${focus * 3}px rgba(235,235,235,${focus * 0.18})`,
      display: "flex",
      alignItems: "center",
      padding: "0 9px",
      fontFamily: MONO,
      fontSize: 12.5,
      color: faint ? D.t3 : D.text,
      whiteSpace: "pre",
      boxSizing: "border-box",
    }}
  >
    {value === "" && placeholder ? <span style={{ color: D.t4 }}>{placeholder}</span> : value}
    {caret}
  </div>
);

const Footer: React.FC<{ b: number }> = ({ b }) => {
  const open = ramp(b, CUE.add + 0.1, 0.45) * (1 - ramp(b, CUE.connect + 0.1, 0.45));
  const text = typed(b, 0);
  const focus = ramp(b, CUE.field, 0.3) * (1 - ramp(b, CUE.connect, 0.3));
  const caret = focus > 0.5 ? <Caret b={b} h={15} /> : null;
  return (
    <div style={{ borderTop: `1px solid ${D.separator}`, height: 46 + 177 * open, position: "relative", overflow: "hidden", fontSize: 12 }}>
      <div style={{ position: "absolute", left: 15, right: 14, top: 0, height: 46, display: "flex", alignItems: "center", opacity: 1 - open }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 13, color: D.text, lineHeight: "16px" }}>Add a host</div>
          <div style={{ fontSize: 12, color: D.t3, lineHeight: "15px" }}>A Mac, Linux or Windows machine you can reach over SSH</div>
        </div>
        <Btn kind="white" pressed={press(b, [CUE.add])}>
          <Icon name="plus" size={13} color={D.onText} />
          Add host
        </Btn>
      </div>
      <div style={{ position: "absolute", inset: 0, opacity: open }}>
        <div style={{ position: "absolute", left: 15, top: 12, fontSize: 13, color: D.text }}>Add a host</div>
        <div style={{ position: "absolute", left: 15, top: 40, color: D.t2 }}>Address</div>
        <div style={{ position: "absolute", left: 547, top: 40, color: D.t2 }}>Port</div>
        <Field x={15} y={59} w={521} value={text} caret={caret} focus={focus} placeholder="10.0.4.12 or devbox.example.com" />
        <Field x={547} y={59} w={79} value="22" faint />
        <div style={{ position: "absolute", left: 15, top: 98, color: D.t2 }}>Username</div>
        <div style={{ position: "absolute", left: 326, top: 98, color: D.t2 }}>Password</div>
        <Field x={15} y={116.5} w={300} value="dev" faint />
        <Field x={326} y={116.5} w={300} value="Only if it refuses your keys" faint />
        <div style={{ position: "absolute", left: 15, top: 155, color: D.t3 }}>
          Signs in with your SSH keys and ~/.ssh/config.{"  "}
          <span style={{ color: D.text, textDecoration: "underline", textUnderlineOffset: 2 }}>Use a key file…</span>
        </div>
        <div style={{ position: "absolute", left: 15, top: 190, color: D.t3 }}>Nothing is changed in your ~/.ssh</div>
        <div style={{ position: "absolute", top: 184, right: 14, display: "flex", gap: 9 }}>
          <Btn style={{ borderRadius: 13 }}>Cancel</Btn>
          <Btn kind="white" pressed={press(b, [CUE.connect])}>Connect</Btn>
        </div>
      </div>
    </div>
  );
};

export const Hosts: React.FC<{ b: number; ms: number }> = ({ b, ms }) => {
  const grow = ramp(b, CUE.connect + 0.1, 0.5);
  const ready = b >= TL.ready;
  return (
    <>
      <SettingsSide />
      <div style={{ position: "absolute", left: 288, top: 11, fontSize: 13, fontWeight: 600, color: D.text }}>Hosts</div>
      <div style={{ position: "absolute", left: SIDE_W, right: 0, top: 41, borderTop: `1px solid ${D.separator}` }} />
      <div style={{ position: "absolute", left: 535, top: 70, width: 40, height: 40, borderRadius: 9, background: D.fill3, display: "grid", placeItems: "center" }}>
        <Icon name="server" size={22} color={D.t2} />
      </div>
      <div style={{ position: "absolute", left: 590, top: 70, fontSize: 20, fontWeight: 700, color: D.text }}>Hosts</div>
      <div style={{ position: "absolute", left: 590, top: 97, fontSize: 12.5, color: D.t2 }}>
        Machines your agents run on: this Mac, and any machine you can sign in to over SSH. Each one is a space in the sidebar.
      </div>
      <div style={{ position: "absolute", left: 539, top: 140, fontSize: 13, fontWeight: 600, color: D.t2 }}>Hosts</div>
      <div style={{ position: "absolute", left: CARD.x, top: CARD.y, width: CARD.w, background: D.surface, borderRadius: 12, border: `1px solid ${D.hairline}`, overflow: "hidden" }}>
        <div style={{ marginTop: -1 }}>
          <Row icon={<MacIcon size={28} />} title="This Mac" line={<><Dot color={D.success} />Connected</>} right={<span style={{ fontSize: 13, color: D.t3 }}>Here</span>} />
          <Row
            icon={<HostIcon color={D.blue} size={28} />}
            title="devbox"
            line={<><Dot color={D.success} />Connected · version 1.4.2 · up 3d</>}
            right={<><Btn>Reconnect</Btn><Btn>Open</Btn><Btn kind="red">Remove</Btn></>}
          />
          {grow > 0 && (
            <div style={{ height: 251 * grow, overflow: "hidden", opacity: interpolate(grow, [0, 0.4], [0, 1], clamp) }}>
              <Row
                icon={<HostIcon color={D.pink} size={28} />}
                title="gpu-box"
                line={ready ? <><Dot color={D.success} />Connected · version 1.4.2 · up 0m</> : <><Dot color={D.waiting} />Connecting…</>}
                right={ready ? <><Btn>Reconnect</Btn><Btn kind="red">Remove</Btn></> : <Btn kind="red">Remove</Btn>}
              >
                <Checklist b={b} ms={ms} />
              </Row>
            </div>
          )}
          <Footer b={b} />
        </div>
      </div>
    </>
  );
};
