// One row's part of the History panel's graph: the lines passing it, the
// lines into and out of its dot, and the dot. Rows grow when a change is
// opened, so each line runs to the row's bottom as a percentage of its height.

import { memo } from "react";

import type { Line, Row, Tone } from "./graph";

/** The width of a lane, and the dot's place from the row's top, in CSS pixels. */
export const LANE = 12;
const EDGE = 7;
const DOT_Y = 15;
const TURN = 8;

const x = (lane: number) => EDGE + lane * LANE;

/** A line's colour class: a branch's, by lane order, or none for one without a name. */
export function toneClass(tone: Tone): string {
  return tone === null ? "tone-none" : `tone-${tone % 4}`;
}

function lineClass(line: Line): string {
  return `${toneClass(line.tone)}${line.dashed ? " dashed" : ""}`;
}

/**
 * One row's cell. Memoized, so choosing a change redraws only the rows whose
 * props changed: `onChoose` must keep its identity between renders.
 */
export const GraphCell = memo(function GraphCell({
  row,
  lanes,
  chosen,
  label,
  onChoose,
}: {
  row: Row;
  lanes: number;
  chosen: boolean;
  /** What the dot selects, for its tooltip. */
  label: string;
  /** Called with the row's changeset ID. */
  onChoose: (id: string) => void;
}) {
  const cx = x(row.lane);
  const choose = () => onChoose(row.id);
  return (
    <div className="history-graph" style={{ width: `${EDGE * 2 + (lanes - 1) * LANE}px` }}>
      <svg aria-hidden="true">
        {row.through.map((line) => (
          <line key={`t${line.lane}${line.tone}${line.dashed}`} className={lineClass(line)} x1={x(line.lane)} y1={0} x2={x(line.lane)} y2="100%" />
        ))}
        {row.into.map((line) =>
          line.lane === row.lane ? (
            <line key={`i${line.lane}${line.tone}${line.dashed}`} className={lineClass(line)} x1={cx} y1={0} x2={cx} y2={DOT_Y} />
          ) : (
            <path
              key={`i${line.lane}${line.tone}${line.dashed}`}
              className={lineClass(line)}
              d={`M ${x(line.lane)} 0 L ${x(line.lane)} ${DOT_Y - TURN} Q ${x(line.lane)} ${DOT_Y} ${cx} ${DOT_Y}`}
            />
          ),
        )}
        {row.out.map((line) =>
          line.lane === row.lane ? (
            <line key={`o${line.lane}${line.tone}${line.dashed}`} className={lineClass(line)} x1={cx} y1={DOT_Y} x2={cx} y2="100%" />
          ) : (
            <g key={`o${line.lane}${line.tone}${line.dashed}`} className={lineClass(line)}>
              <path d={`M ${cx} ${DOT_Y} Q ${x(line.lane)} ${DOT_Y} ${x(line.lane)} ${DOT_Y + TURN}`} />
              <line x1={x(line.lane)} y1={DOT_Y + TURN} x2={x(line.lane)} y2="100%" />
            </g>
          ),
        )}
        {/* A bigger target than the dot itself. */}
        <circle className="history-hit" cx={cx} cy={DOT_Y} r={LANE / 2} onClick={choose} />
        <circle
          className={`history-dot ${toneClass(row.tone)}${row.hollow ? " hollow" : ""}${chosen ? " chosen" : ""}`}
          data-id={row.id}
          cx={cx}
          cy={DOT_Y}
          r={row.hollow ? 4 : 4.5}
          onClick={choose}
        >
          <title>{label}</title>
        </circle>
      </svg>
    </div>
  );
});
