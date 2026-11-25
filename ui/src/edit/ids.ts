// The Activity ID a new activity gets: the next free one following the
// pattern of its WBS band's IDs (a prefix and a numbered step, such as
// RC1350 then RC1360), or the schedule's where the band has none. Only a
// suggestion: the planner can type another before adding it.

/** An ID read as a prefix and a number, keeping the number's width. */
interface Numbered {
  prefix: string;
  number: number;
  width: number;
}

function numbered(code: string): Numbered | null {
  const found = /^(.*?)(\d+)$/.exec(code);
  if (!found) return null;
  const [, prefix = "", digits = ""] = found;
  return { prefix, number: Number(digits), width: digits.length };
}

/** The pattern a set of IDs follows: their most common prefix, its last number, step and width. */
function patternOf(codes: readonly string[]): { prefix: string; last: number; step: number; width: number; gaps: number } | null {
  const parsed = codes.map(numbered).filter((n): n is Numbered => n !== null);
  if (parsed.length === 0) return null;
  // The prefix most IDs share; on a tie, the one the last ID has.
  const counts = new Map<string, number>();
  for (const n of parsed) counts.set(n.prefix, (counts.get(n.prefix) ?? 0) + 1);
  let most = 0;
  for (const n of counts.values()) most = Math.max(most, n);
  const prefix = [...parsed].reverse().find((n) => counts.get(n.prefix) === most)?.prefix ?? "";
  const same = parsed.filter((n) => n.prefix === prefix);
  const numbers = [...new Set(same.map((n) => n.number))].sort((a, b) => a - b);
  // The step most often between neighbours; on a tie, the smallest. One ID
  // alone steps by 10 when it's a round number, else by 1, where nothing
  // else gives a step.
  const gaps = new Map<number, number>();
  for (let k = 1; k < numbers.length; k++) {
    const gap = (numbers[k] ?? 0) - (numbers[k - 1] ?? 0);
    gaps.set(gap, (gaps.get(gap) ?? 0) + 1);
  }
  let step = (numbers[0] ?? 0) % 10 === 0 ? 10 : 1;
  let often = 0;
  for (const [gap, n] of gaps) {
    if (n > often || (n === often && gap < step)) [often, step] = [n, gap];
  }
  let width = 0;
  for (const n of same) width = Math.max(width, n.width);
  return { prefix, last: numbers.at(-1) ?? 0, step, width, gaps: gaps.size };
}

/**
 * The next free Activity ID after `band`'s, by its pattern; where it has no
 * numbered IDs, by the whole schedule's; with none anywhere, A1000 and on in
 * tens. Never one in `taken` (case aside).
 */
export function nextId(band: readonly string[], all: readonly string[]): string {
  const own = patternOf(band);
  const whole = patternOf(all);
  // One ID in the band has no step of its own: it takes the schedule's, where they share a prefix.
  const borrow = own && own.gaps === 0 && whole && whole.gaps > 0 && whole.prefix === own.prefix;
  const pattern = (borrow ? { ...own, step: whole.step } : own) ??
    whole ?? { prefix: "A", last: 990, step: 10, width: 4, gaps: 0 };
  const taken = new Set(all.map((code) => code.toLowerCase()));
  let number = pattern.last + pattern.step;
  for (;;) {
    const code = `${pattern.prefix}${String(number).padStart(pattern.width, "0")}`;
    if (!taken.has(code.toLowerCase())) return code;
    number += pattern.step;
  }
}
