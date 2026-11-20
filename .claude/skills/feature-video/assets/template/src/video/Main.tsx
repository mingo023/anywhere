import React from "react";
import { interpolate } from "remotion";
import { Caret, D, Dot, Icon, MONO, Spinner, Sweep, TL, clamp, outExpo, press, ramp, typed } from "./kit";
import { SIDE_W } from "./Sidebar";

export const CUE = { folder: 25, code: 26, repo: 27, use: 28, composer: 29.3, send: 34, live: 35, task: 40 };
const PROMPT = TL.typing[1].text;
const TITLE = "Train on today's data";

const Crumbs: React.FC<{ parts: string[] }> = ({ parts }) => (
  <div style={{ position: "absolute", left: 284, top: 11, display: "flex", gap: 8, fontSize: 13 }}>
    {parts.map((p, i) => (
      <React.Fragment key={p}>
        {i > 0 && <span style={{ color: D.t5 }}>/</span>}
        <span style={{ color: i === parts.length - 1 ? D.text : D.t3, fontWeight: i === parts.length - 1 ? 600 : 400 }}>{p}</span>
      </React.Fragment>
    ))}
  </div>
);

const Rail: React.FC = () => (
  <>
    {(
      [
        ["comment", 20],
        ["copy", 59],
        ["branch", 96],
        ["github", 134],
      ] as const
    ).map(([n, y]) => (
      <Icon key={n} name={n} size={15} color={D.t2} style={{ position: "absolute", left: 1417 - 7.5, top: y - 7.5 }} />
    ))}
  </>
);

const Pill: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={{ height: 28, padding: "0 10px", borderRadius: 7, background: D.fill2, display: "flex", alignItems: "center", gap: 7, fontSize: 13, color: D.text }}>{children}</div>
);

const Chev = () => <Icon name="chevron-down" size={12} color={D.t3} />;

const Finder: React.FC<{ b: number }> = ({ b }) => {
  const shown = ramp(b, CUE.folder + 0.1, 0.35) * (1 - ramp(b, CUE.use + 0.1, 0.3));
  if (shown <= 0) return null;
  const code = b >= CUE.code;
  const repo = b >= CUE.repo;
  const row = (y: number, x: number, w: number, icon: string, label: string, sel: boolean, arrow = false) => (
    <div
      key={label}
      style={{ position: "absolute", left: x, width: w, top: y - 13, height: 26, borderRadius: 6, background: sel ? D.fill4 : undefined, display: "flex", alignItems: "center", gap: 8, padding: "0 8px", boxSizing: "border-box", fontSize: 13, color: D.text }}
    >
      <Icon name={icon} size={14} color={D.t2} />
      <span style={{ flex: 1 }}>{label}</span>
      {arrow && <Icon name="chevron-right" size={12} color={D.t4} />}
    </div>
  );
  const appear = (c: number) => ({ opacity: ramp(b, c + 0.05, 0.3), translate: `${(1 - ramp(b, c + 0.05, 0.3)) * 8}px 0` });
  return (
    <div style={{ position: "absolute", inset: 0, background: `rgba(0,0,0,${0.3 * shown})` }}>
      <div
        style={{
          position: "absolute",
          left: 230,
          top: 140,
          width: 980,
          height: 620,
          borderRadius: 14,
          background: D.popover,
          border: `1px solid ${D.sepStrong}`,
          boxShadow: "0 30px 80px rgba(0,0,0,0.5)",
          overflow: "hidden",
          opacity: shown,
          scale: 0.96 + 0.04 * shown,
        }}
      >
        <div style={{ position: "absolute", left: 15, top: 16, fontSize: 13, color: D.t3, display: "flex", gap: 10 }}>
          <span>Choose a folder</span>
          <span style={{ color: D.t2 }}>{repo ? "~/code/ml-trainer" : code ? "~/code" : "~"}</span>
        </div>
        <Icon name="eye-off" size={15} color={D.t3} style={{ position: "absolute", left: 776, top: 18 }} />
        <div style={{ position: "absolute", left: 808, top: 12, height: 26, padding: "0 9px", borderRadius: 8, background: D.fill3, display: "flex", alignItems: "center", gap: 7, fontSize: 13, color: D.text }}>
          <Icon name="server" size={13} color={D.text} />
          gpu-box
          <Dot color={D.success} />
          <Chev />
        </div>
        <Icon name="x" size={15} color={D.t2} style={{ position: "absolute", left: 944, top: 18 }} />
        <div style={{ position: "absolute", left: 0, right: 0, top: 51, borderTop: `1px solid ${D.separator}` }} />
        <div style={{ position: "absolute", left: 0, top: 52, width: 180, bottom: 56, background: D.fill1, borderRight: `1px solid ${D.separator}` }}>
          <div style={{ position: "absolute", left: 16, top: 18, fontSize: 11, fontWeight: 600, color: D.t3, letterSpacing: "0.04em" }}>GPU-BOX</div>
          {row(48, 8, 164, "laptop", "Home", true)}
          {row(77, 8, 164, "server", "Root", false)}
        </div>
        <div style={{ position: "absolute", left: 181, top: 52, width: 218, bottom: 56, borderRight: `1px solid ${D.separator}` }}>
          {row(17, 6, 206, "folder", "code", code, true)}
          {row(43, 6, 206, "folder", "datasets", false, true)}
          {row(69, 6, 206, "folder", "Library", false, true)}
          {row(95, 6, 206, "file", "notes.md", false)}
        </div>
        {code && (
          <div style={{ position: "absolute", left: 400, top: 52, width: 218, bottom: 56, borderRight: `1px solid ${D.separator}`, ...appear(CUE.code) }}>
            {row(17, 6, 206, "folder", "ml-trainer", repo, true)}
            {row(43, 6, 206, "folder", "infra", false, true)}
            {row(69, 6, 206, "folder", "notebooks", false, true)}
          </div>
        )}
        {repo && (
          <div style={{ position: "absolute", left: 619, right: 0, top: 52, bottom: 56, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 8, ...appear(CUE.repo) }}>
            <div style={{ width: 64, height: 64, borderRadius: 14, background: D.fill3, display: "grid", placeItems: "center" }}>
              <Icon name="folder" size={30} color={D.t2} />
            </div>
            <div style={{ fontSize: 14, fontWeight: 600, color: D.text }}>ml-trainer</div>
            <div style={{ fontSize: 12, color: D.t3 }}>Git repository · main</div>
          </div>
        )}
        <div style={{ position: "absolute", left: 0, right: 0, bottom: 56, borderTop: `1px solid ${D.separator}` }} />
        <div style={{ position: "absolute", left: 15, bottom: 12, fontSize: 12 }}>
          <div style={{ color: D.text, fontFamily: MONO }}>{repo ? "~/code/ml-trainer" : code ? "~/code" : "~"}</div>
          <div style={{ color: D.t3, display: "flex", alignItems: "center", gap: 5, marginTop: 2 }}>
            <Icon name={repo ? "branch" : "folder"} size={11} color={D.t3} />
            {repo ? "Git repository, on main" : "Folder, without git"}
          </div>
        </div>
        <div style={{ position: "absolute", right: 14, bottom: 14, display: "flex", gap: 9 }}>
          <div style={{ height: 30, padding: "0 13px", borderRadius: 15, background: D.fill4, color: D.text, fontSize: 13, display: "flex", alignItems: "center" }}>Cancel</div>
          <div
            style={{
              height: 30,
              padding: "0 13px",
              borderRadius: 15,
              background: D.text,
              color: D.onText,
              fontSize: 13,
              fontWeight: 600,
              display: "flex",
              alignItems: "center",
              scale: 1 - press(b, [CUE.use]) * 0.06,
            }}
          >
            Use this folder
          </div>
        </div>
      </div>
    </div>
  );
};

const Composer: React.FC<{ b: number; ms: number; chat: number }> = ({ b, ms, chat }) => {
  const text = typed(b, 1);
  const focus = b >= CUE.composer && b < CUE.send;
  const sent = b >= CUE.send + 0.1;
  const [x0, y0, x1, y1] = [interpolate(chat, [0, 1], [534, 382]), interpolate(chat, [0, 1], [421, 743]), interpolate(chat, [0, 1], [1133, 1285]), interpolate(chat, [0, 1], [541, 860])];
  const ready = text.length > 0 && !sent;
  return (
    <div
      style={{
        position: "absolute",
        left: x0,
        top: y0,
        width: x1 - x0,
        height: y1 - y0,
        borderRadius: 15,
        background: D.glass,
        border: `1px solid ${focus ? D.sepStrong : D.separator}`,
        boxShadow: focus ? "0 0 0 3px rgba(235,235,235,0.06)" : undefined,
      }}
    >
      <div style={{ position: "absolute", left: 20, top: 18, fontSize: 14, color: sent ? D.t3 : D.text, whiteSpace: "pre" }}>
        {sent ? "Send a follow-up into this turn…" : text === "" ? <span style={{ color: D.t3 }}>Ask, build, / for commands, @ for files</span> : text}
        {focus && <Caret b={b} />}
      </div>
      <div style={{ position: "absolute", left: 16, right: 14, bottom: 13, height: 30, display: "flex", alignItems: "center", gap: 7, fontSize: 13, color: D.text }}>
        <Icon name="claude" size={14} color={D.claude} />
        {chat > 0.5 ? (
          <>
            <span style={{ fontWeight: 600 }}>Opus</span>
            <span style={{ color: D.t3 }}>High</span>
            <Chev />
          </>
        ) : (
          <>
            Claude Code <Chev />
            <Icon name="shield" size={13} color={D.t2} style={{ marginLeft: 10 }} />
            Agent's settings <Chev />
          </>
        )}
        <span style={{ flex: 1 }} />
        <Icon name="plus" size={16} color={D.t2} style={{ marginRight: 12 }} />
        <div
          style={{
            width: 32,
            height: 32,
            borderRadius: 16,
            background: ready || sent ? D.text : D.fill3,
            display: "grid",
            placeItems: "center",
            scale: 1 - press(b, [CUE.send]) * 0.12,
          }}
        >
          {sent ? <span style={{ width: 11, height: 11, borderRadius: 2.5, background: D.onText }} /> : <Icon name="arrow-up" size={15} color={ready ? D.onText : D.t5} />}
        </div>
      </div>
    </div>
  );
};

const Task: React.FC<{ label: string; state: "done" | "active" | "todo"; ms: number }> = ({ label, state, ms }) => (
  <div style={{ height: 30, display: "flex", alignItems: "center", gap: 10, padding: "0 16px", fontSize: 13 }}>
    {state === "done" ? (
      <span style={{ width: 14, height: 14, borderRadius: 7, background: "rgba(48,164,108,0.2)", display: "grid", placeItems: "center" }}>
        <Icon name="check" size={10} color={D.successText} />
      </span>
    ) : state === "active" ? (
      <Spinner ms={ms} size={14} color="#70b8ff" />
    ) : (
      <span style={{ width: 12, height: 12, borderRadius: 7, border: `1.5px solid ${D.t4}`, margin: 1 }} />
    )}
    <span style={{ color: state === "done" ? D.t4 : D.text, textDecoration: state === "done" ? "line-through" : undefined }}>{label}</span>
  </div>
);

const Loss: React.FC<{ b: number; at: number; from: number; to: number; epoch: number }> = ({ b, at, from, to, epoch }) => {
  const p = ramp(b, at, 0.8);
  return (
    <div style={{ opacity: ramp(b, at, 0.2), color: D.t2 }}>
      epoch {epoch}/3{"   "}loss <span style={{ color: D.successText }}>{(from + (to - from) * p).toFixed(3)}</span>
    </div>
  );
};

/** A block that grows to `h` and fades up from beat `at`. */
const Grow: React.FC<{ b: number; at: number; h: number; children: React.ReactNode }> = ({ b, at, h, children }) => {
  const p = ramp(b, at, 0.45);
  return <div style={{ height: (h + 14) * p, opacity: p, translate: `0 ${(1 - p) * 10}px`, overflow: p < 1 ? "hidden" : undefined }}>{children}</div>;
};

const Chat: React.FC<{ b: number; ms: number }> = ({ b, ms }) => {
  const secs = Math.max(0, Math.floor(((b - CUE.live) * 60) / TL.bpm));
  const ran = b >= 38.5;
  return (
    <div style={{ position: "absolute", left: 394, width: 879, top: 56 }}>
      <div style={{ display: "flex", justifyContent: "flex-end", opacity: ramp(b, CUE.send + 0.15, 0.4), translate: `0 ${(1 - ramp(b, CUE.send + 0.15, 0.4)) * 16}px` }}>
        <div style={{ background: D.bubble, color: D.text, borderRadius: 18, padding: "9px 15px", fontSize: 13.5 }}>{PROMPT}</div>
      </div>
      <div style={{ height: 12 }} />
      <div style={{ height: 34, display: "flex", justifyContent: "flex-end", gap: 10, alignItems: "flex-start", fontSize: 11, color: D.t4, opacity: ramp(b, CUE.send + 0.4, 0.4) }}>
        <Icon name="copy" size={12} color={D.t4} />
        10:04
      </div>
      <Grow b={b} at={36} h={22}>
        <div style={{ fontSize: 13.5, lineHeight: "22px", color: D.body }}>I'll check the training script, then run it here on gpu-box.</div>
      </Grow>
      <Grow b={b} at={37} h={24}>
        <div style={{ height: 24, display: "flex", alignItems: "center", gap: 8, fontSize: 13, color: D.t3, paddingLeft: 6 }}>
          <Icon name={ran ? "terminal" : "search"} size={13} color={D.t3} />
          {ran ? "Explored the project · Ran 1 command" : "Explored the project"}
        </div>
      </Grow>
      <Grow b={b} at={38} h={136}>
        <div style={{ height: 136, borderRadius: 10, background: D.fill1, border: `1px solid ${D.separator}`, boxSizing: "border-box" }}>
          <div style={{ height: 38, display: "flex", alignItems: "center", gap: 9, padding: "0 16px", fontSize: 13, fontWeight: 600, color: D.text }}>
            <Icon name="list" size={13} color={D.t2} />
            Tasks
            <span style={{ flex: 1 }} />
            <span style={{ fontFamily: MONO, fontSize: 11, fontWeight: 400, color: D.t3, background: D.fill3, borderRadius: 9, padding: "2px 8px" }}>{b >= CUE.task ? "1" : "0"} of 3</span>
          </div>
          <Task label="Load today's data" state={b >= CUE.task ? "done" : "active"} ms={ms} />
          <Task label="Train 3 epochs" state={b >= CUE.task ? "active" : "todo"} ms={ms} />
          <Task label="Report the eval loss" state="todo" ms={ms} />
        </div>
      </Grow>
      <Grow b={b} at={39.5} h={112}>
        <div style={{ height: 112, borderRadius: 10, background: "rgba(235,235,235,0.04)", border: "1px solid #323232", boxSizing: "border-box", padding: "10px 14px", fontFamily: MONO, fontSize: 12.5, lineHeight: "20px" }}>
          <div style={{ display: "flex", fontFamily: "inherit", fontSize: 11, color: D.t3, marginBottom: 4 }}>
            <span style={{ flex: 1 }}>bash · gpu-box</span>
            <Icon name="copy" size={12} color={D.t4} />
          </div>
          <div style={{ color: D.text }}>
            <span style={{ color: D.t3 }}>$ </span>python train.py <span style={{ color: D.synKw }}>--data</span> today <span style={{ color: D.synKw }}>--epochs</span> 3
          </div>
          <Loss b={b} at={41} from={0.921} to={0.412} epoch={1} />
          <Loss b={b} at={42.5} from={0.412} to={0.287} epoch={2} />
        </div>
      </Grow>
      <div style={{ height: 24, display: "flex", alignItems: "center", gap: 8, fontSize: 13, opacity: ramp(b, CUE.live, 0.3) }}>
        <Icon name="claude" size={14} color={D.claude} />
        <Sweep text={`Opus 5.5 working for ${secs}s`} ms={ms} base={D.t3} />
      </div>
    </div>
  );
};

/** Everything right of the sidebar while a space is open: New session on gpu-box, the folder picker, then the chat. */
const Pills: React.FC<{ host: React.ReactNode; project: React.ReactNode }> = ({ host, project }) => (
  <div style={{ position: "absolute", left: 534, top: 552, display: "flex", alignItems: "center", gap: 6 }}>
    <Pill>
      {host}
      <Chev />
    </Pill>
    <span style={{ color: D.t5 }}>·</span>
    <Pill>
      {project}
      <Chev />
    </Pill>
    <span style={{ color: D.t5 }}>·</span>
    <Pill>
      <Icon name="worktree" size={13} color={D.t2} />
      Worktree
      <Chev />
    </Pill>
    <span style={{ color: D.t5 }}>·</span>
    <Pill>
      <Icon name="branch" size={13} color={D.t2} />
      <span style={{ fontFamily: MONO }}>main</span>
      <Chev />
    </Pill>
  </div>
);

const Badge: React.FC<{ letter: string }> = ({ letter }) => (
  <span style={{ width: 14, height: 14, borderRadius: 3, background: D.fill3, color: D.t2, fontSize: 8, fontWeight: 600, display: "grid", placeItems: "center" }}>{letter}</span>
);

const Heading = () => (
  <div style={{ position: "absolute", left: 534, width: 599, top: 377, textAlign: "center", fontSize: 20, fontWeight: 600, color: D.text }}>What should we build next?</div>
);

const Dots: React.FC<{ opacity: number }> = ({ opacity }) => (
  <div
    style={{
      position: "absolute",
      inset: 0,
      opacity,
      backgroundImage: "radial-gradient(circle, rgba(235,235,235,0.07) 0.8px, transparent 1.2px)",
      backgroundSize: "14px 14px",
    }}
  />
);

/** Switching space lands on New session in that space's first project (`switch_space` → `new_place`). */
const MacHome: React.FC<{ ms: number }> = ({ ms }) => (
  <div style={{ position: "absolute", inset: 0 }}>
    <Crumbs parts={["app-android", "New session"]} />
    <Heading />
    <Composer b={0} ms={ms} chat={0} />
    <Pills
      host={
        <>
          <Icon name="laptop" size={13} color={D.t2} />
          This Mac
        </>
      }
      project={
        <>
          <Badge letter="A" />
          app-android
        </>
      }
    />
  </div>
);

export const Main: React.FC<{ b: number; ms: number; mac?: number }> = ({ b, ms, mac = 0 }) => {
  const project = b >= CUE.use + 0.1;
  const chat = ramp(b, CUE.send + 0.05, 0.5, outExpo);
  const home = 1 - interpolate(chat, [0, 0.35], [0, 1], clamp);
  return (
    <div style={{ position: "absolute", left: SIDE_W, right: 0, top: 0, bottom: 0 }}>
      <Dots opacity={Math.max(home, mac)} />
      <div style={{ position: "absolute", inset: 0, left: -SIDE_W, opacity: 1 - mac }}>
        <div style={{ opacity: home }}>
          <Crumbs parts={project ? ["gpu-box", "ml-trainer", "New session"] : ["gpu-box", "New session"]} />
          <Heading />
          <Pills
            host={
              <>
                <Icon name="server" size={13} color={D.t2} />
                gpu-box
                <Dot color={D.success} />
              </>
            }
            project={
              project ? (
                <>
                  <Badge letter="T" />
                  ml-trainer
                </>
              ) : (
                <span style={{ color: D.t3 }}>No project</span>
              )
            }
          />
        </div>
        {chat > 0 && (
          <div style={{ opacity: interpolate(chat, [0.2, 0.6], [0, 1], clamp) }}>
            <div style={{ position: "absolute", left: 278, top: 5, width: 224, height: 30, borderRadius: 8, background: D.fill2, display: "flex", alignItems: "center", gap: 9, padding: "0 12px", boxSizing: "border-box" }}>
              <Icon name="claude" size={13} color={D.claude} />
              <div style={{ lineHeight: "12px" }}>
                <div style={{ fontSize: 12.5, color: D.text }}>{TITLE}</div>
                <div style={{ fontSize: 10.5, color: D.t3 }}>Claude Code · gpu-box</div>
              </div>
            </div>
            <Icon name="plus" size={14} color={D.t3} style={{ position: "absolute", left: 512, top: 13 }} />
            <Chat b={b} ms={ms} />
            <div style={{ position: "absolute", left: 394, top: 868, display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: D.t3, fontFamily: MONO }}>
              <Icon name="worktree" size={12} color={D.t3} />
              main
            </div>
            <div style={{ position: "absolute", right: 1440 - 1277, top: 868, fontSize: 11.5, color: D.t4 }}>Working · esc to stop</div>
          </div>
        )}
        <Composer b={b} ms={ms} chat={chat} />
        <Finder b={b} />
      </div>
      <div style={{ position: "absolute", inset: 0, left: -SIDE_W }}>
        <Rail />
        {mac > 0 && (
          <div style={{ opacity: mac }}>
            <MacHome ms={ms} />
          </div>
        )}
      </div>
    </div>
  );
};

