// The project in the title block: its name and code, as a button that opens
// the project picker. Serving a portfolio, the picker is the portfolio's
// projects (the EPS, each project's figures and the capacity chart), and a
// project chosen there is shown. Serving one file, it lists that file's
// projects.

import { type ReactNode, useEffect, useRef } from "react";

import type { Summary } from "../api";
import { readable } from "../explain/narrate";
import { Icon } from "./icons";

export function ProjectPicker({
  summary,
  open,
  onOpen,
  portfolio,
}: {
  summary: Summary | null;
  open: boolean;
  onOpen: (open: boolean) => void;
  /** The portfolio's projects, when a portfolio is served. */
  portfolio?: ReactNode;
}) {
  const button = useRef<HTMLButtonElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const projects = summary?.inspect.projects ?? [];
  const first = projects[0];
  const others = projects.length - 1;

  // A click or focus anywhere else closes it; Escape closes it and gives focus back.
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      const target = event.target as Node;
      if (!popup.current?.contains(target) && !button.current?.contains(target)) onOpen(false);
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("focusin", away);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("focusin", away);
    };
  }, [open, onOpen]);

  // Focus goes into the picker as it opens: its first button, or the picker
  // itself. Closed with focus inside, as by its key, focus goes back to the button.
  useEffect(() => {
    if (open) {
      (popup.current?.querySelector<HTMLElement>("button") ?? popup.current)?.focus();
      return;
    }
    if (document.activeElement === document.body) button.current?.focus();
  }, [open]);

  const name = first?.name || first?.short_name || (summary ? "Untitled schedule" : "Loading…");
  const code = first?.name && first.name !== first.short_name ? first.short_name : null;
  const more = others > 0 ? `and ${others} more ${others === 1 ? "project" : "projects"}` : null;

  return (
    <div className="project-picker">
      <button
        ref={button}
        type="button"
        className="project"
        aria-haspopup="dialog"
        aria-expanded={open}
        // The whole name and code, which a narrow window cuts short, then the file.
        title={[[name, code, more].filter(Boolean).join(" "), summary?.location].filter(Boolean).join("\n")}
        onClick={() => onOpen(!open)}
      >
        <span className="project-name">{name}</span>
        {code && <span className="project-code">{code}</span>}
        {more && <span className="project-code">{more}</span>}
        <Icon name="down" size={14} className="tb-quiet" />
      </button>
      {open && (
        <div
          ref={popup}
          className="project-popup"
          role="dialog"
          aria-label="Projects"
          tabIndex={-1}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              onOpen(false);
              button.current?.focus();
            }
          }}
        >
          {portfolio ?? (
            <section className="project-list" aria-label="Projects in this file">
              <h2>Projects in this schedule</h2>
              <p className="muted project-file">{summary?.location}</p>
              <ul>
                {projects.map((p) => (
                  <li key={p.id}>
                    <span className="project-list-name">
                      {p.name || p.short_name} {p.name && <span className="muted">{p.short_name}</span>}
                    </span>
                    <span className="muted">
                      {p.activities.toLocaleString()} {p.activities === 1 ? "activity" : "activities"} · data date{" "}
                      {p.data_date ? readable(p.data_date, false) : "none"} · finish {p.finish ? readable(p.finish, false) : "none"}
                    </span>
                  </li>
                ))}
              </ul>
              <p className="muted">Serve a portfolio file to move between schedules here.</p>
            </section>
          )}
        </div>
      )}
    </div>
  );
}
