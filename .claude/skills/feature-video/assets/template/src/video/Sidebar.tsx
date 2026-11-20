import React from "react";
import { D, Dot, GradientSpinner, Icon, Sweep } from "./kit";

// Window points, read off the dark 2x captures (preview px × 1.2).
export const SIDE_W = 271;

const Row: React.FC<{ y: number; h?: number; left?: number; right?: number; children: React.ReactNode; style?: React.CSSProperties }> = ({
  y,
  h = 26,
  left = 0,
  right = 0,
  children,
  style,
}) => (
  <div style={{ position: "absolute", top: y - h / 2, height: h, left, right, display: "flex", alignItems: "center", ...style }}>{children}</div>
);

const Label: React.FC<{ y: number; text: string; icons?: string[] }> = ({ y, text, icons = [] }) => (
  <Row y={y} left={14} right={14}>
    <span style={{ flex: 1, fontSize: 11, fontWeight: 600, letterSpacing: "0.04em", color: D.t3 }}>{text}</span>
    <div style={{ display: "flex", gap: 10 }}>
      {icons.map((i) => (
        <Icon key={i} name={i} size={14} color={D.t3} />
      ))}
    </div>
  </Row>
);

const Badge: React.FC<{ letter: string }> = ({ letter }) => (
  <span style={{ width: 16, height: 16, borderRadius: 4, background: D.fill3, color: D.t2, fontSize: 9, fontWeight: 600, display: "grid", placeItems: "center", flex: "none" }}>
    {letter}
  </span>
);

const Project: React.FC<{ y: number; name: string }> = ({ y, name }) => (
  <Row y={y} left={15}>
    <Badge letter={name.split("-").pop()![0].toUpperCase()} />
    <span style={{ marginLeft: 9, fontSize: 13, color: D.text }}>{name}</span>
  </Row>
);

/**
 * The sidebar labels a session by its title, as `tree_label` does once a session has a name.
 * @throws when `name` looks like a branch (`fix/restore-handoff`): take the title from `.ui-review/fixture/fixture.ts`.
 */
const title = (name: string) => {
  if (/^[\w.-]+\/[\w./-]+$/.test(name)) throw new Error(`Sidebar row "${name}" is a branch; use the session's title from .ui-review/fixture/fixture.ts`);
  return name;
};

const Tree: React.FC<{ y: number; name: string; glyph: React.ReactNode; selected?: boolean; working?: boolean; ms: number; diff?: [number, number] }> = ({
  y,
  name,
  glyph,
  selected,
  working,
  ms,
  diff,
}) => (
  <Row y={y} h={26} left={8} right={9} style={{ borderRadius: 6, background: selected ? D.fill3 : undefined }}>
    <span style={{ width: 32, paddingRight: 12, display: "flex", justifyContent: "flex-end" }}>{glyph}</span>
    <span style={{ flex: 1, minWidth: 0, fontSize: 13, color: selected ? D.text : D.body, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
      {working ? <Sweep text={title(name)} ms={ms} base={selected ? D.text : D.body} /> : title(name)}
    </span>
    {diff && (
      <span style={{ fontSize: 11, display: "flex", gap: 6, paddingRight: 4 }}>
        <span style={{ color: D.successText }}>+{diff[0]}</span>
        <span style={{ color: D.failedText }}>−{diff[1]}</span>
      </span>
    )}
  </Row>
);

const Halo: React.FC = () => (
  <span style={{ width: 13, height: 13, borderRadius: 7, background: D.waitingBg, display: "grid", placeItems: "center" }}>
    <Dot color={D.waiting} size={7} />
  </span>
);

const HostIcon: React.FC<{ color: string; size?: number }> = ({ color, size = 20 }) => (
  <span style={{ width: size, height: size, borderRadius: size * 0.27, background: color, display: "grid", placeItems: "center", flex: "none" }}>
    <Icon name="server" size={size * 0.62} color="#fff" />
  </span>
);

export const MacIcon: React.FC<{ size?: number }> = ({ size = 20 }) => (
  <span style={{ width: size, height: size, borderRadius: size * 0.27, background: D.fill4, display: "grid", placeItems: "center", flex: "none" }}>
    <Icon name="laptop" size={size * 0.62} color={D.t2} />
  </span>
);

export type Space = "mac" | "gpu";

export type SidebarState = {
  space: Space;
  /** 0→1 as the space's list slides in. */
  slide: number;
  /** Whether gpu-box is a host yet. */
  gpuHost: boolean;
  /** 0→1 as ml-trainer joins gpu-box's projects. */
  project: number;
  /** 0→1 as the gpu-box session row appears. */
  session: number;
  ms: number;
};

const MacList: React.FC<{ ms: number }> = ({ ms }) => (
  <>
    <Label y={234} text="SESSIONS" icons={["plus"]} />
    <Label y={276} text="PROJECTS" icons={["search", "arrow-down-wide-narrow", "folder-plus"]} />
    <Project y={307} name="app-android" />
    <Tree y={337} name="Fix stale terminal reveal" glyph={<GradientSpinner ms={ms} size={11} />} working ms={ms} diff={[42, 17]} />
    <Tree y={366} name="Split restore hook into two files" glyph={<Icon name="worktree" size={12} color={D.t4} />} ms={ms} />
    <Tree y={395} name="Crash on resume after OTA" glyph={<Icon name="worktree" size={12} color={D.t4} />} ms={ms} diff={[84, 51]} />
    <Tree y={424} name="Add haptics to approval sheet" glyph={<Icon name="worktree" size={12} color={D.t4} />} ms={ms} diff={[84, 51]} />
    <Project y={460} name="app-ios" />
    <Tree y={489} name="Fix keyboard inset" glyph={<Halo />} ms={ms} />
    <Project y={525} name="app-hook" />
    <Tree y={554} name="Retry webhook deliveries" glyph={<GradientSpinner ms={ms} size={11} />} working ms={ms} />
    <Project y={590} name="marketing" />
  </>
);

const GpuList: React.FC<{ project: number; session: number; ms: number }> = ({ project, session, ms }) => (
  <>
    <Label y={234} text="PROJECTS" icons={["search", "arrow-down-wide-narrow", "folder-plus"]} />
    <div style={{ position: "absolute", inset: 0, opacity: 1 - project }}>
      <div style={{ position: "absolute", top: 270, left: 0, right: 0, display: "flex", flexDirection: "column", alignItems: "center", textAlign: "center" }}>
        <Icon name="folder" size={19} color={D.t3} />
        <div style={{ marginTop: 9, fontSize: 13, fontWeight: 500, color: D.text }}>No projects on gpu-box yet</div>
        <div style={{ marginTop: 6, fontSize: 12, lineHeight: "15px", color: D.t3 }}>
          Open a folder that's already there, or
          <br />
          clone a repository onto it.
        </div>
      </div>
      <div style={{ position: "absolute", top: 364, left: 48, display: "flex", gap: 6 }}>
        <span style={{ height: 24, padding: "0 11px", borderRadius: 12, background: D.fill4, color: D.text, fontSize: 12.5, display: "flex", alignItems: "center" }}>Open folder…</span>
        <span style={{ height: 24, padding: "0 11px", borderRadius: 12, background: D.fill4, color: D.text, fontSize: 12.5, display: "flex", alignItems: "center" }}>Clone…</span>
      </div>
    </div>
    <div style={{ position: "absolute", inset: 0, opacity: project, translate: `0 ${(1 - project) * 8}px` }}>
      <Project y={265} name="ml-trainer" />
      <div style={{ position: "absolute", inset: 0, opacity: session, translate: `0 ${(1 - session) * 6}px` }}>
        <Tree y={294} name="Train on today's data" glyph={<GradientSpinner ms={ms} size={11} />} working selected ms={ms} />
      </div>
    </div>
  </>
);

export const Sidebar: React.FC<SidebarState & { pressedIcon?: number }> = ({ space, slide, gpuHost, project, session, ms, pressedIcon = 0 }) => {
  const host =
    space === "mac" ? (
      <>
        <MacIcon />
        <span style={{ marginLeft: 9, fontSize: 14, fontWeight: 600, color: D.text }}>This Mac</span>
      </>
    ) : (
      <>
        <HostIcon color={D.pink} />
        <span style={{ marginLeft: 9, fontSize: 14, fontWeight: 600, color: D.text }}>gpu-box</span>
        <Dot color={D.success} style={{ marginLeft: 7 }} />
        <span style={{ marginLeft: 6, fontSize: 12, color: D.t3 }}>gpu-box</span>
      </>
    );
  const dir = space === "gpu" ? 1 : -1;
  const sel = space === "mac" ? 25 : 94;
  return (
    <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: SIDE_W, borderRight: `1px solid ${D.separator}` }}>
      <div style={{ position: "absolute", left: 14, top: 14, display: "flex", gap: 8 }}>
        {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
          <Dot key={c} color={c} size={12} />
        ))}
      </div>
      <Icon name="bell" size={15} color={D.t2} style={{ position: "absolute", left: 214, top: 13 }} />
      <span style={{ position: "absolute", left: 223, top: 6, minWidth: 13, height: 13, borderRadius: 7, background: D.waiting, color: D.onText, fontSize: 9, fontWeight: 700, display: "grid", placeItems: "center" }}>1</span>
      <Icon name="sidebar-collapse" size={15} color={D.t2} style={{ position: "absolute", left: 243, top: 13 }} />
      <Row y={58} h={30} left={12} right={11} style={{ borderRadius: 8, background: D.fill2, padding: "0 10px" }}>
        <Icon name="search" size={14} color={D.t3} />
        <span style={{ marginLeft: 11, flex: 1, fontSize: 13, color: D.t3 }}>Search all machines</span>
        <span style={{ fontSize: 12, color: D.t4 }}>⌘K</span>
      </Row>
      {[
        ["activity", "Activity", 94],
        ["blocks", "Board", 124],
        ["bolt", "Automations", 154],
      ].map(([i, l, y]) => (
        <Row key={l as string} y={y as number} left={16}>
          <Icon name={i as string} size={16} color={D.t2} />
          <span style={{ marginLeft: 9, fontSize: 13, color: D.body }}>{l}</span>
        </Row>
      ))}
      <div style={{ position: "absolute", inset: 0, opacity: slide, translate: `${(1 - slide) * 24 * dir}px 0` }}>
        <Row y={193} left={15} right={14}>
          {host}
          <span style={{ flex: 1 }} />
          <Icon name="chevron-down" size={14} color={D.t3} />
        </Row>
        {space === "mac" ? <MacList ms={ms} /> : <GpuList project={project} session={session} ms={ms} />}
      </div>
      <Row y={872} left={10} right={14}>
        <div style={{ position: "absolute", left: sel - 10 - 15, width: 30, height: 28, borderRadius: 7, background: D.fill3, scale: 1 - pressedIcon * 0.1 }} />
        {[
          { x: 25, el: <Icon name="laptop" size={17} color={space === "mac" ? D.text : D.t2} />, dot: undefined, badge: true },
          { x: 59, el: <Icon name="server" size={17} color={D.t2} />, dot: D.success },
          ...(gpuHost ? [{ x: 94, el: <Icon name="server" size={17} color={space === "gpu" ? D.text : D.t2} />, dot: D.success }] : []),
        ].map((h) => (
          <div key={h.x} style={{ position: "absolute", left: h.x - 10 - 8.5, display: "flex" }}>
            {h.el}
            {h.dot && <Dot color={h.dot} size={6} style={{ position: "absolute", right: -2, bottom: -1, boxShadow: "0 0 0 2px #151515" }} />}
            {h.badge && (
              <span style={{ position: "absolute", right: -7, top: -7, minWidth: 13, height: 13, borderRadius: 7, background: D.waiting, color: D.onText, fontSize: 9, fontWeight: 700, display: "grid", placeItems: "center" }}>1</span>
            )}
          </div>
        ))}
        <Icon name="plus" size={16} color={D.t2} style={{ position: "absolute", left: (gpuHost ? 128 : 94) - 10 - 8 }} />
        <Icon name="settings" size={16} color={D.t2} style={{ position: "absolute", left: 247 - 10 - 8 }} />
      </Row>
    </div>
  );
};

export { HostIcon };
