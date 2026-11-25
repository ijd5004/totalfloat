// The app's icons: a small set of line icons drawn here as inline SVG, so
// nothing is fetched or bundled from a package (ARCHITECTURE.md, Local app). Each takes the
// colour of its text. An icon beside words is decorative and hidden from
// screen readers; one standing alone is named by `label`.

import type { ReactNode } from "react";

const PATHS = {
  down: <path d="M6 9l6 6 6-6" />,
  right: <path d="M9 6l6 6-6 6" />,
  plus: <path d="M12 5v14M5 12h14" />,
  search: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="M20 20l-3.5-3.5" />
    </>
  ),
  branch: (
    <>
      <circle cx="6" cy="18" r="2.5" />
      <circle cx="6" cy="6" r="2.5" />
      <circle cx="18" cy="7" r="2.5" />
      <path d="M6 8.5v7M18 9.5c0 4-4 5-9.5 7" />
    </>
  ),
  x: <path d="M17 7L7 17M7 7l10 10" />,
  menu: <path d="M4 7h16M4 12h16M4 17h16" />,
  link: (
    <>
      <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
      <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
    </>
  ),
  drive: <path d="M3 17h5l3-9 3 9h7" />,
  table: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M3 10h18M9 10v10" />
    </>
  ),
  group: <path d="M4 6h16M7 12h13M10 18h10" />,
  panel: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M15 4v16" />
    </>
  ),
  review: (
    <>
      <path d="M4 5h16v11H9l-5 4z" />
      <path d="M9 10l2 2 4-4" />
    </>
  ),
} as const;

export type IconName = keyof typeof PATHS;

/** A line icon, 16 px unless `size` says otherwise. */
export function Icon({ name, size = 16, label, className }: { name: IconName; size?: number; label?: string; className?: string }) {
  return (
    <svg
      className={`icon${className ? ` ${className}` : ""}`}
      viewBox="0 0 24 24"
      width={size}
      height={size}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      {PATHS[name] as ReactNode}
    </svg>
  );
}

/** TotalFloat's mark: three bars, the middle one hi-vis, on a slate tile. */
export function Mark() {
  return (
    <span className="mark" aria-hidden="true">
      <svg width="15" height="15" viewBox="0 0 16 16" focusable="false">
        <rect x="1.5" y="3" width="8" height="2.4" rx="1" className="mark-bar" />
        <rect x="5" y="7" width="9.5" height="2.4" rx="1" className="mark-hivis" />
        <rect x="3" y="11" width="6" height="2.4" rx="1" className="mark-bar" />
      </svg>
    </span>
  );
}
