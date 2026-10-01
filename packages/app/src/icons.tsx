import React from "react";
import Svg, { Circle, Path, Rect } from "react-native-svg";

export type IconProps = { size: number; color: string };

function Icon({
  size,
  color,
  strokeWidth = 2,
  children,
}: IconProps & { strokeWidth?: number; children: React.ReactNode }) {
  return (
    <Svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </Svg>
  );
}

export const ChevronLeft = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M15 18l-6-6 6-6" />
  </Icon>
);

export const ChevronDown = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M6 9l6 6 6-6" />
  </Icon>
);

export const ChevronRight = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M9 6l6 6-6 6" />
  </Icon>
);

export const GitBranch = (p: IconProps) => (
  <Icon {...p}>
    <Circle cx="6" cy="5" r="2" />
    <Circle cx="6" cy="19" r="2" />
    <Circle cx="18" cy="7" r="2" />
    <Path d="M6 7v10M18 9c0 5-6 4-12 8" />
  </Icon>
);

export const Terminal = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M4 17l6-5-6-5M12 19h8" />
  </Icon>
);

export const Pencil = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M4 20h4L19 9l-4-4L4 16v4z" />
  </Icon>
);

export const Target = (p: IconProps) => (
  <Icon {...p}>
    <Circle cx="12" cy="12" r="3.5" />
    <Path d="M3 12h5.5M15.5 12H21" />
  </Icon>
);

export const File = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M7 3h7l5 5v13H7z" />
    <Path d="M14 3v5h5" />
  </Icon>
);

export const Search = (p: IconProps) => (
  <Icon {...p}>
    <Circle cx="11" cy="11" r="6" />
    <Path d="M20 20l-4.6-4.6" />
  </Icon>
);

export const Check = (p: IconProps) => (
  <Icon {...p} strokeWidth={2.4}>
    <Path d="M5 12l5 5 9-10" />
  </Icon>
);

export const Cross = (p: IconProps) => (
  <Icon {...p} strokeWidth={2.4}>
    <Path d="M6 6l12 12M18 6L6 18" />
  </Icon>
);

export const ArrowUp = (p: IconProps) => (
  <Icon {...p} strokeWidth={2.4}>
    <Path d="M12 19V5M5 12l7-7 7 7" />
  </Icon>
);

export const Stop = (p: IconProps) => (
  <Icon {...p}>
    <Rect x="7" y="7" width="10" height="10" rx="2" fill={p.color} />
  </Icon>
);

export const Minus = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M5 12h14" />
  </Icon>
);

export const List = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />
  </Icon>
);

export const Bulb = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M9 18h6M10 21h4M12 3a6 6 0 00-3 11.2V16h6v-1.8A6 6 0 0012 3z" />
  </Icon>
);

export const CircleOutline = (p: IconProps) => (
  <Icon {...p}>
    <Circle cx="12" cy="12" r="8" />
  </Icon>
);

export const Asterisk = (p: IconProps) => (
  <Icon {...p}>
    <Path d="M12 3v18M4.2 7.5l15.6 9M19.8 7.5l-15.6 9" />
  </Icon>
);
