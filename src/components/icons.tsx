// Small inline SVG icons for the redesign (One Piece "Navires" theme).
// React components so we don't depend on a global <symbol> sheet.
import type { CSSProperties } from "react";

interface IconProps {
  size?: number;
  className?: string;
  color?: string;
  style?: CSSProperties;
}

const stroke = (size = 16, color = "currentColor", className?: string, style?: CSSProperties) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: color,
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  className,
  style,
});

export function Sword({ size = 16, className, color = "#FF7062", style }: IconProps) {
  return (
    <svg {...stroke(size, color, className, style)} strokeWidth={2.2}>
      <path d="M14.5 17.5 3 6V3h3l11.5 11.5" />
      <path d="M13 19l6-6" />
      <path d="M16 16l4 4" />
      <path d="M19 21l2-2" />
    </svg>
  );
}

export function Shield({ size = 16, className, color = "#7FB0E8", style }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill={color} className={className} style={style}>
      <path d="M12 2 4 5v6c0 5 3.4 9 8 11 4.6-2 8-6 8-11V5z" />
    </svg>
  );
}

export function Heart({ size = 16, className, color = "#5BC46A", style }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill={color} className={className} style={style}>
      <path d="M12 21s-7-4.3-9.5-8.6C.7 9.1 2.3 5 6 5c2 0 3.2 1.1 4 2.3C10.8 6.1 12 5 14 5c3.7 0 5.3 4.1 3.5 7.4C19 16.7 12 21 12 21z" />
    </svg>
  );
}

export function Bolt({ size = 16, className, color = "#E8B84B", style }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill={color} className={className} style={style}>
      <path d="M13 2 4 14h7l-1 8 9-12h-7z" />
    </svg>
  );
}

export function Hat({ size = 16, className, color = "currentColor", style }: IconProps) {
  return (
    <svg {...stroke(size, color, className, style)} fill="none">
      <ellipse cx="12" cy="15" rx="10" ry="3.2" />
      <path d="M6 15c0-4 2.7-7 6-7s6 3 6 7" />
      <path d="M6.5 13.4h11" />
    </svg>
  );
}

export function Anchor({ size = 16, className, color = "currentColor", style }: IconProps) {
  return (
    <svg {...stroke(size, color, className, style)}>
      <circle cx="12" cy="4.5" r="2.5" />
      <line x1="12" y1="22" x2="12" y2="7" />
      <path d="M5 12H2.5a9.5 9.5 0 0 0 19 0H19" />
      <line x1="7" y1="10" x2="17" y2="10" />
    </svg>
  );
}

export function SkullCross({ size = 16, className, color = "currentColor", style }: IconProps) {
  return (
    <svg {...stroke(size, color, className, style)}>
      <circle cx="12" cy="9" r="5.5" />
      <circle cx="10" cy="8.5" r="1" />
      <circle cx="14" cy="8.5" r="1" />
      <path d="M9 12.5h6" />
      <path d="M4 18l16-3M4 15l16 3" />
    </svg>
  );
}

export function Crest({ which, size = 16, className, color = "currentColor", style }: IconProps & { which: "hat" | "anchor" }) {
  return which === "anchor"
    ? <Anchor size={size} className={className} color={color} style={style} />
    : <Hat size={size} className={className} color={color} style={style} />;
}
