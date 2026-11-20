import { Composition, Still } from "remotion";
import { AppWindow, TOTAL, Video } from "./video/Video";
import { FPB } from "./video/kit";

export const RemotionRoot: React.FC = () => (
  <>
    <Composition id="Video" component={Video} durationInFrames={TOTAL} fps={60} width={1920} height={1080} />
    {/* The window alone at 1440×900 points over black, to compare with a capture flattened on black (scripts/pair.py). */}
    <Still
      id="Window"
      component={({ beat }: { beat: number }) => (
        <div style={{ width: 1440, height: 900, background: "#000", position: "relative" }}>
          <div style={{ position: "absolute", inset: 0, background: "rgba(18,18,18,0.85)" }} />
          <AppWindow b={beat} ms={(beat * FPB * 1000) / 60} />
        </div>
      )}
      width={1440}
      height={900}
      defaultProps={{ beat: 10 }}
    />
  </>
);
