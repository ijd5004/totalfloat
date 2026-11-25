//! Map local calendar definitions to effective model calendars without I/O.

use std::collections::BTreeMap;

use totalfloat_model::{Calendar, CalendarId, CalendarType, HoursPerPeriod, ProjectId, WorkWeek};

use super::{CalendarData, DiagnosticKind};
use crate::typed::{CalendarRow, Cell, CellState, Decimal, RowRef, TypedTable, TypedXer};

/// A calendar warning with original raw coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowDiagnostic {
    /// Raw source row.
    pub source: RowRef,
    /// Warning category.
    pub kind: DiagnosticKind,
    /// Description without private payload values.
    pub message: &'static str,
}

/// Every input calendar row, including malformed or unresolved rows.
#[derive(Debug)]
pub struct CalendarEntry<'a> {
    /// Original raw row coordinates.
    pub source: RowRef,
    /// Local definitions and exact unmodified source encoding.
    pub data: CalendarData<'a>,
    /// Effective model calendar, or `None` when a safe interpretation is unavailable.
    pub calendar: Option<Calendar>,
}

/// Resolved calendars and warnings, retaining every input row's text.
#[derive(Debug)]
pub struct CalendarSet<'a> {
    /// Entries in source order.
    pub entries: Vec<CalendarEntry<'a>>,
    /// Parsing, metadata and inheritance warnings in deterministic order.
    pub diagnostics: Vec<RowDiagnostic>,
}

impl CalendarSet<'_> {
    /// The first successfully resolved calendar with this identity.
    #[must_use]
    pub fn calendar(&self, id: CalendarId) -> Option<&Calendar> {
        self.entries
            .iter()
            .filter_map(|entry| entry.calendar.as_ref())
            .find(|calendar| calendar.id == id)
    }
}

/// Decode every calendar row, then resolve base inheritance independent of row order.
///
/// Missing bases, cycles, incomplete weeks and malformed local rules do not get
/// fabricated work patterns. Their model is `None` and their original text remains.
#[must_use]
pub fn decode_calendars<'a>(typed: &TypedXer<'a>) -> CalendarSet<'a> {
    let mut entries = Vec::new();
    let mut seeds = Vec::new();
    let mut diagnostics = Vec::new();
    let mut ids = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::Calendar { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let data = CalendarData::decode(row.clndr_data.original().unwrap_or(""));
            for diagnostic in data.diagnostics() {
                diagnostics.push(RowDiagnostic {
                    source: row.meta.source,
                    kind: diagnostic.kind,
                    message: diagnostic.message,
                });
            }
            let mut seed = metadata(row, &mut diagnostics);
            if let Some(calendar) = &seed {
                if let std::collections::btree_map::Entry::Vacant(entry) = ids.entry(calendar.id) {
                    entry.insert(entries.len());
                } else {
                    warn(
                        &mut diagnostics,
                        row.meta.source,
                        DiagnosticKind::DuplicateId,
                        "duplicate calendar identity; first definition used",
                    );
                    seed = None;
                }
            }
            seeds.push(seed);
            entries.push(CalendarEntry {
                source: row.meta.source,
                data,
                calendar: None,
            });
        }
    }
    let mut state = vec![0u8; entries.len()];
    let mut resolved = vec![None; entries.len()];
    let mut heights = vec![0usize; entries.len()];
    let mut dates = vec![None; entries.len()];
    let mut traversal = Resolver {
        entries: &entries,
        seeds: &seeds,
        ids: &ids,
        state: &mut state,
        resolved: &mut resolved,
        diagnostics: &mut diagnostics,
        heights: &mut heights,
        dates: &mut dates,
    };
    for index in 0..entries.len() {
        traversal.resolve(index);
    }
    for (entry, calendar) in entries.iter_mut().zip(resolved) {
        entry.calendar = calendar;
    }
    CalendarSet {
        entries,
        diagnostics,
    }
}

fn warn(out: &mut Vec<RowDiagnostic>, source: RowRef, kind: DiagnosticKind, message: &'static str) {
    out.push(RowDiagnostic {
        source,
        kind,
        message,
    });
}

fn period(
    cell: &Cell<'_, Decimal>,
    source: RowRef,
    out: &mut Vec<RowDiagnostic>,
) -> Option<totalfloat_model::Minutes> {
    if let Some(value) = cell.value() {
        if let Some(minutes) = value.export_minutes().filter(|m| m.get() > 0) {
            return Some(minutes);
        }
        warn(
            out,
            source,
            DiagnosticKind::InvalidHoursPerPeriod,
            "conversion factor is not positive whole-minute hours",
        );
    } else if matches!(cell.state(), CellState::Invalid(_)) {
        warn(
            out,
            source,
            DiagnosticKind::InvalidHoursPerPeriod,
            "invalid conversion factor retained raw",
        );
    }
    None
}

fn metadata(row: &CalendarRow<'_>, out: &mut Vec<RowDiagnostic>) -> Option<Calendar> {
    let source = row.meta.source;
    let id = row
        .clndr_id
        .value()
        .copied()
        .filter(|id| *id > 0)
        .and_then(|id| u64::try_from(id).ok());
    let kind = match row.clndr_type.value().copied() {
        Some("CA_Base") => Some(CalendarType::Global),
        Some("CA_Project") => Some(CalendarType::Project),
        Some("CA_Rsrc") => Some(CalendarType::Resource),
        _ => None,
    };
    let (Some(id), Some(kind)) = (id, kind) else {
        warn(
            out,
            source,
            DiagnosticKind::InvalidMetadata,
            "calendar identity or scope is missing or invalid",
        );
        return None;
    };
    let optional_id = |cell: &Cell<'_, i64>| {
        cell.value()
            .copied()
            .filter(|id| *id > 0)
            .and_then(|id| u64::try_from(id).ok())
    };
    if matches!(row.base_clndr_id.state(), CellState::Invalid(_))
        || row.base_clndr_id.value().is_some_and(|id| *id <= 0)
    {
        warn(
            out,
            source,
            DiagnosticKind::InvalidMetadata,
            "base calendar identity is invalid",
        );
        return None;
    }
    Some(Calendar {
        id: CalendarId(id),
        name: row.clndr_name.value().copied().unwrap_or("").to_owned(),
        kind,
        project: optional_id(&row.proj_id).map(ProjectId),
        base: optional_id(&row.base_clndr_id).map(CalendarId),
        is_default: row.default_flag.value().copied().unwrap_or(false),
        hours_per_period: HoursPerPeriod {
            day: period(&row.day_hr_cnt, source, out),
            week: period(&row.week_hr_cnt, source, out),
            month: period(&row.month_hr_cnt, source, out),
            year: period(&row.year_hr_cnt, source, out),
        },
        work_week: WorkWeek::default(),
        exceptions: BTreeMap::new(),
        source: source.to_source_ref(),
    })
}

struct Resolver<'a, 'b> {
    entries: &'b [CalendarEntry<'a>],
    seeds: &'b [Option<Calendar>],
    ids: &'b BTreeMap<CalendarId, usize>,
    state: &'b mut [u8],
    resolved: &'b mut [Option<Calendar>],
    diagnostics: &'b mut Vec<RowDiagnostic>,
    heights: &'b mut [usize],
    dates:
        &'b mut [Option<BTreeMap<totalfloat_model::Date, Vec<totalfloat_model::WorkInterval>>>],
}

impl Resolver<'_, '_> {
    fn resolve(&mut self, index: usize) {
        if self.state[index] == 2 {
            return;
        }
        // Walk iteratively: untrusted base chains never recurse on the call stack.
        let mut trail = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut cursor = index;
        while self.state[cursor] != 2 {
            if !seen.insert(cursor) {
                warn(
                    self.diagnostics,
                    self.entries[cursor].source,
                    DiagnosticKind::InheritanceCycle,
                    "base calendar chain cycles",
                );
                for index in trail {
                    self.state[index] = 2;
                }
                return;
            }
            trail.push(cursor);
            let Some(base) = self.seeds[cursor].as_ref().and_then(|seed| seed.base) else {
                break;
            };
            let Some(&parent) = self.ids.get(&base) else {
                break;
            };
            cursor = parent;
        }
        for index in trail.into_iter().rev() {
            self.resolved[index] = self.build(index);
            self.state[index] = 2;
        }
    }

    fn build(&mut self, index: usize) -> Option<Calendar> {
        let mut calendar = self.seeds[index].clone()?;
        if !self.entries[index].data.is_valid() {
            return None;
        }
        let source = self.entries[index].source;
        let mut height = 1;
        let base = if let Some(id) = calendar.base {
            let Some(&base_index) = self.ids.get(&id) else {
                warn(
                    self.diagnostics,
                    source,
                    DiagnosticKind::MissingBase,
                    "referenced base calendar is absent",
                );
                return None;
            };
            let Some(base) = self.dates[base_index].clone() else {
                warn(
                    self.diagnostics,
                    source,
                    DiagnosticKind::MissingBase,
                    "base exception definitions cannot be resolved safely",
                );
                return None;
            };
            height = self.heights[base_index] + 1;
            if height > 64 {
                warn(
                    self.diagnostics,
                    source,
                    DiagnosticKind::InheritanceCycle,
                    "base calendar chain exceeds 64 levels",
                );
                return None;
            }
            Some(base)
        } else {
            None
        };
        let data = &self.entries[index].data;
        let mut exceptions = base.unwrap_or_default();
        exceptions.extend(data.exceptions.clone());
        // A missing local standard week does not erase valid inherited date rules.
        self.dates[index] = Some(exceptions.clone());
        self.heights[index] = height;
        for (day, local) in data.week.iter().enumerate() {
            if let Some(intervals) = local {
                calendar.work_week.0[day].clone_from(intervals);
            } else {
                warn(
                    self.diagnostics,
                    source,
                    DiagnosticKind::UnresolvedWeek,
                    "standard weekday has no local definition; base inheritance covers exceptions",
                );
                return None;
            }
        }
        calendar.exceptions = exceptions;
        let weekly_minutes: i64 = calendar
            .work_week
            .0
            .iter()
            .flatten()
            .map(|interval| interval.duration().get())
            .sum();
        if calendar
            .hours_per_period
            .week
            .is_some_and(|week| week.get() != weekly_minutes)
        {
            warn(
                self.diagnostics,
                source,
                DiagnosticKind::WeeklyHoursMismatch,
                "stored display week hours differ from standard weekly intervals",
            );
        }
        Some(calendar)
    }
}
