//! Decode `CALENDAR.clndr_data` without losing its original spelling.
//!
//! The nested format carries weekday intervals and serial-day exceptions.
//! Parsing is bounded and nonfatal; incomplete data must not silently become
//! a usable all-nonworking calendar. See TF-011 for evidence and limitations.

use std::collections::BTreeMap;

use totalfloat_model::{Date, TimeOfDay, Weekday, WorkInterval};

use crate::typed::Decimal;

mod syntax;
use syntax::Node;

/// Calendar parsing/resolution warning category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticKind {
    /// Parentheses/header syntax is malformed or parser limits were exceeded.
    Syntax,
    /// Unknown or duplicated structural section.
    UnsupportedStructure,
    /// Weekday number is invalid or occurs more than once.
    InvalidWeekday,
    /// Shift times are missing, invalid, reversed or overlap.
    InvalidInterval,
    /// Exception serial date is invalid or out of civil-date range.
    InvalidExceptionDate,
    /// A date has repeated definitions; identical ones are harmless, conflicts are not.
    DuplicateException,
    /// Required calendar metadata is absent or invalid.
    InvalidMetadata,
    /// A referenced base calendar is absent.
    MissingBase,
    /// Base-calendar references cycle or exceed the resolution depth limit.
    InheritanceCycle,
    /// A standard weekday has no local definition; exceptions do not supply one.
    UnresolvedWeek,
    /// Display conversion factor cannot be represented in exact minutes.
    InvalidHoursPerPeriod,
    /// Stored display week hours differ from actual standard weekly work time.
    WeeklyHoursMismatch,
    /// Calendar identity is duplicated in the file.
    DuplicateId,
}

/// One warning without private calendar content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Warning category.
    pub kind: DiagnosticKind,
    /// Fixed description; original payload values are never embedded.
    pub message: &'static str,
}

/// Local definitions before inheritance. All parsed state is read-only.
#[derive(Debug, Clone)]
pub struct CalendarData<'a> {
    original: &'a str,
    week: [Option<Vec<WorkInterval>>; 7],
    exceptions: BTreeMap<Date, Vec<WorkInterval>>,
    diagnostics: Vec<Diagnostic>,
    valid: bool,
}

impl<'a> CalendarData<'a> {
    /// Decodes a calendar tree; malformed input remains available with diagnostics.
    #[must_use]
    pub fn decode(source: &'a str) -> Self {
        let mut result = Self {
            original: source,
            week: std::array::from_fn(|_| None),
            exceptions: BTreeMap::new(),
            diagnostics: Vec::new(),
            valid: true,
        };
        // An empty payload has no local definitions; resolution reports missing weeks.
        if source.trim().is_empty() {
            return result;
        }
        match syntax::parse(source) {
            Ok(root) => result.root(&root),
            Err(message) => result.warn(DiagnosticKind::Syntax, message),
        }
        result
    }

    /// Exact unmodified source text, including quotes, indentation and DEL bytes.
    #[must_use]
    pub const fn encode(&self) -> &'a str {
        self.original
    }

    /// Local definition: `None` is unspecified; an empty slice explicitly means no work.
    #[must_use]
    pub fn weekday(&self, day: Weekday) -> Option<&[WorkInterval]> {
        self.week[day.index()].as_deref()
    }

    /// Local date overrides; an empty list explicitly makes the date nonworking.
    #[must_use]
    pub const fn exceptions(&self) -> &BTreeMap<Date, Vec<WorkInterval>> {
        &self.exceptions
    }

    /// All parsing warnings. Resolution and metadata warnings are separate.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the local syntax/values are supported (inheritance may still fail).
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.valid
    }

    fn warn(&mut self, kind: DiagnosticKind, message: &'static str) {
        self.valid = false;
        self.diagnostics.push(Diagnostic { kind, message });
    }

    fn root(&mut self, root: &Node<'_>) {
        // An unnamed root is observed in otherwise complete resource calendars.
        if !matches!(root.name, "CalendarData" | "") || !root.payload.trim().is_empty() {
            self.warn(
                DiagnosticKind::UnsupportedStructure,
                "unrecognized calendar root",
            );
            return;
        }
        let (mut week, mut exceptions) = (false, false);
        for section in &root.children {
            match section.name {
                "DaysOfWeek" if !week => {
                    week = true;
                    self.week_section(section);
                }
                "Exceptions" if !exceptions => {
                    exceptions = true;
                    self.exception_section(section);
                }
                // VIEW/ShowTotal is display metadata; it has no work intervals.
                "VIEW" if section.children.is_empty() => {}
                _ => self.warn(
                    DiagnosticKind::UnsupportedStructure,
                    "unknown or duplicate calendar section",
                ),
            }
        }
    }

    fn week_section(&mut self, section: &Node<'_>) {
        if !section.payload.trim().is_empty() {
            self.warn(
                DiagnosticKind::UnsupportedStructure,
                "unexpected weekday section payload",
            );
        }
        for day in &section.children {
            let Ok(day_number) = day.name.parse::<usize>() else {
                self.warn(DiagnosticKind::InvalidWeekday, "invalid weekday number");
                continue;
            };
            if !(1..=7).contains(&day_number) || !day.payload.trim().is_empty() {
                self.warn(DiagnosticKind::InvalidWeekday, "invalid weekday definition");
                continue;
            }
            let index = day_number - 1;
            if self.week[index].is_some() {
                self.warn(
                    DiagnosticKind::InvalidWeekday,
                    "duplicate weekday definition",
                );
                continue;
            }
            match intervals(&day.children) {
                Ok(intervals) => self.week[index] = Some(intervals),
                Err(message) => self.warn(DiagnosticKind::InvalidInterval, message),
            }
        }
    }

    fn exception_section(&mut self, section: &Node<'_>) {
        if !section.payload.trim().is_empty() {
            self.warn(
                DiagnosticKind::UnsupportedStructure,
                "unexpected exception section payload",
            );
        }
        for exception in &section.children {
            let Some(date) = exception_date_payload(exception.payload) else {
                self.warn(
                    DiagnosticKind::InvalidExceptionDate,
                    "unsupported exception serial day",
                );
                continue;
            };
            let Ok(intervals) = intervals(&exception.children) else {
                self.warn(
                    DiagnosticKind::InvalidInterval,
                    "invalid exception work intervals",
                );
                continue;
            };
            if let Some(identical) = self.exceptions.get(&date).map(|old| *old == intervals) {
                if identical {
                    self.diagnostics.push(Diagnostic {
                        kind: DiagnosticKind::DuplicateException,
                        message: "identical exception date repeated; one model override retained",
                    });
                } else {
                    self.warn(
                        DiagnosticKind::DuplicateException,
                        "conflicting exception date",
                    );
                }
            } else {
                self.exceptions.insert(date, intervals);
            }
        }
    }
}

/// TF-011: civil serial day zero is 1899-12-30, with no Excel leap-day adjustment.
#[must_use]
pub fn exception_date(serial_day: i32) -> Option<Date> {
    Date::from_ymd(1899, 12, 30)?.checked_add_days(serial_day)
}

// Anonymization also checks ignored display metadata and node labels: the
// general calendar decoder deliberately accepts those without interpreting them.
pub(crate) fn contains_only_pattern_metadata(source: &str) -> bool {
    fn known(node: &Node<'_>) -> bool {
        let name_ok = matches!(
            node.name,
            "" | "CalendarData" | "DaysOfWeek" | "Exceptions" | "VIEW"
        ) || node.name.parse::<u32>().is_ok();
        let payload_ok = node.payload.split('|').all(|part| {
            matches!(part, "s" | "f" | "d" | "ShowTotal")
                || part.bytes().all(|byte| {
                    byte.is_ascii_digit()
                        || byte.is_ascii_whitespace()
                        || matches!(byte, b':' | b'.' | b'-' | b'+')
                })
        });
        name_ok && payload_ok && node.children.iter().all(known)
    }
    source.trim().is_empty() || syntax::parse(source).is_ok_and(|root| known(&root))
}

fn exception_date_payload(payload: &str) -> Option<Date> {
    let (key, value) = payload.split_once('|')?;
    if key != "d" {
        return None;
    }
    let serial: Decimal = value.parse().ok()?;
    exception_date(i32::try_from(serial.checked_integer()?).ok()?)
}

fn intervals(nodes: &[Node<'_>]) -> Result<Vec<WorkInterval>, &'static str> {
    let mut result = Vec::new();
    for node in nodes {
        if !node.children.is_empty() {
            return Err("nested work shift is unsupported");
        }
        let mut parts = node.payload.split('|');
        let (mut start, mut finish) = (None, None);
        while let Some(key) = parts.next() {
            let value = parts.next().ok_or("unpaired shift key")?;
            match key {
                "s" if start.is_none() => start = Some(time(value).ok_or("invalid shift start")?),
                "f" if finish.is_none() => {
                    finish = Some(time(value).ok_or("invalid shift finish")?);
                }
                _ => return Err("unknown or duplicate shift key"),
            }
        }
        // TF-011: midnight in the finish position denotes the end of the day.
        let finish = finish.and_then(|time| {
            if time.minutes() == 0 {
                TimeOfDay::from_minutes(1440)
            } else {
                Some(time)
            }
        });
        let interval = start
            .zip(finish)
            .and_then(|(s, f)| WorkInterval::new(s, f))
            .ok_or("missing or reversed shift interval")?;
        result.push(interval);
    }
    result.sort();
    if result.windows(2).any(|pair| pair[0].finish > pair[1].start) {
        return Err("overlapping shift intervals");
    }
    Ok(result)
}

fn time(source: &str) -> Option<TimeOfDay> {
    let (hour, minute) = source.split_once(':')?;
    if !(1..=2).contains(&hour.len())
        || minute.len() != 2
        || !hour
            .bytes()
            .chain(minute.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    TimeOfDay::from_hm(hour.parse().ok()?, minute.parse().ok()?)
}

mod resolve;
pub use resolve::{CalendarEntry, CalendarSet, RowDiagnostic, decode_calendars};

#[cfg(test)]
mod tests;
