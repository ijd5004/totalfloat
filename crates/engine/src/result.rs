//! Engine output, in the fields XER files store.

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_model::{ActivityId, DateTime, Minutes, ProjectId, RelationshipId, StoredResults};

use crate::{DateField, DateProvenance};

/// The result of scheduling one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleResult {
    /// Project-level results.
    pub project: ProjectResult,
    /// Results per activity.
    pub activities: BTreeMap<ActivityId, ActivityResult>,
}

/// Project-level results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectResult {
    /// The project.
    pub id: ProjectId,
    /// Data date used. XER: `PROJECT.last_recalc_date`.
    pub data_date: Option<DateTime>,
    /// Scheduled finish. XER: `PROJECT.scd_end_date`. With float from the
    /// opened projects' finish, the latest finish of the projects open together.
    pub scheduled_finish: Option<DateTime>,
    /// The latest early finish of this project's own work, before
    /// as-late-as-possible activities move: where its longest path ends. The
    /// same as `scheduled_finish` unless float runs from the opened projects'
    /// finish (TF-030). Not stored in XER files.
    pub network_finish: Option<DateTime>,
}

/// Results for one activity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityResult {
    /// Dates, float and float paths, in the same shape a file's stored results map
    /// to, so the two compare directly. A field is `None` where it doesn't
    /// apply, such as the remaining dates of a completed activity.
    pub values: StoredResults,
    /// Critical under the project's definition. `None` when unknown: XER files don't
    /// store this flag, so results read back from a file have no value.
    pub critical: Option<bool>,
    /// Relationships with no slack into this activity, in ascending ID order. This
    /// traces logic for the longest path, so it can list a link from finished work
    /// even when `provenance` shows the data date set the date (TF-026).
    pub driving_relationships: Vec<RelationshipId>,
    /// Actual start, from the input. XER: `TASK.act_start_date`.
    pub actual_start: Option<DateTime>,
    /// Actual finish, from the input. XER: `TASK.act_end_date`.
    pub actual_finish: Option<DateTime>,
    /// What controlled each computed date in `values`.
    pub provenance: BTreeMap<DateField, DateProvenance>,
    /// Total float with every link inverted exactly, where that differs from
    /// `values.total_float`, which can lose time a lag calendar works through
    /// (TF-032).
    pub true_total_float: Option<Minutes>,
}

impl ActivityResult {
    /// The value of one computed date.
    #[must_use]
    pub fn date(&self, field: DateField) -> Option<DateTime> {
        match field {
            DateField::EarlyStart => self.values.early_start,
            DateField::EarlyFinish => self.values.early_finish,
            DateField::LateStart => self.values.late_start,
            DateField::LateFinish => self.values.late_finish,
            DateField::RemainingEarlyStart => self.values.remaining_early_start,
            DateField::RemainingEarlyFinish => self.values.remaining_early_finish,
            DateField::RemainingLateStart => self.values.remaining_late_start,
            DateField::RemainingLateFinish => self.values.remaining_late_finish,
            DateField::ActualStart => self.actual_start,
            DateField::ActualFinish => self.actual_finish,
        }
    }
}

/// One step of a driving chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainLink<'a> {
    /// The activity.
    pub activity: ActivityId,
    /// Which of its dates.
    pub field: DateField,
    /// The date's value, if the activity is in this result.
    pub date: Option<DateTime>,
    /// What controlled it. `None` if unrecorded, for example at an activity in
    /// another project.
    pub provenance: Option<&'a DateProvenance>,
}

impl ScheduleResult {
    /// Walks backwards from a date through what drove it, across relationships
    /// and within activities, starting with that date and ending at what
    /// started the chain.
    ///
    /// Stops if a date repeats, so a malformed result can't loop forever.
    #[must_use]
    pub fn driving_chain(&self, activity: ActivityId, field: DateField) -> Vec<ChainLink<'_>> {
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut next = Some((activity, field));
        while let Some((activity, field)) = next.take() {
            if !seen.insert((activity, field)) {
                break;
            }
            let result = self.activities.get(&activity);
            let provenance = result.and_then(|r| r.provenance.get(&field));
            chain.push(ChainLink {
                activity,
                field,
                date: result.and_then(|r| r.date(field)),
                provenance,
            });
            next = match provenance.map(DateProvenance::driver) {
                Some(DateProvenance::Relationship {
                    from, from_date, ..
                }) => Some((*from, *from_date)),
                Some(DateProvenance::Duration { from_date }) => Some((activity, *from_date)),
                _ => None,
            };
        }
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use totalfloat_model::{CalendarId, LagCalendar, RelationshipType};

    const A: ActivityId = ActivityId(1);
    const B: ActivityId = ActivityId(2);
    const C: ActivityId = ActivityId(3);

    fn at(text: &str) -> DateTime {
        text.parse().unwrap()
    }

    fn link(id: u64, from: ActivityId, from_date: DateField) -> DateProvenance {
        DateProvenance::Relationship {
            relationship: RelationshipId(id),
            from,
            from_date,
            kind: RelationshipType::FinishToStart,
            lag: Minutes(0),
            lag_calendar: LagCalendar::Predecessor,
        }
    }

    fn activity(early_start: &str, field: DateField, why: DateProvenance) -> ActivityResult {
        let mut result = ActivityResult::default();
        result.values.early_start = Some(at(early_start));
        result.provenance.insert(field, why);
        result
    }

    /// A starts on the data date and its finish follows from that. B starts
    /// after A finishes, moved to the next working day. C starts with B.
    fn chain_of_three() -> ScheduleResult {
        let mut a = activity(
            "2026-01-05 08:00",
            DateField::EarlyStart,
            DateProvenance::DataDate,
        );
        a.values.early_finish = Some(at("2026-01-05 17:00"));
        a.provenance.insert(
            DateField::EarlyFinish,
            DateProvenance::CalendarAdjustment {
                unadjusted: at("2026-01-05 16:00"),
                calendar: CalendarId(1),
                cause: Box::new(DateProvenance::Duration {
                    from_date: DateField::EarlyStart,
                }),
            },
        );
        let mut b = activity(
            "2026-01-06 08:00",
            DateField::EarlyStart,
            DateProvenance::CalendarAdjustment {
                unadjusted: at("2026-01-05 17:00"),
                calendar: CalendarId(1),
                cause: Box::new(link(1, A, DateField::EarlyFinish)),
            },
        );
        b.driving_relationships.push(RelationshipId(1));
        let mut c = activity(
            "2026-01-07 08:00",
            DateField::EarlyStart,
            link(2, B, DateField::EarlyStart),
        );
        c.driving_relationships.push(RelationshipId(2));
        ScheduleResult {
            project: ProjectResult {
                id: ProjectId(1),
                data_date: Some(at("2026-01-05 08:00")),
                scheduled_finish: Some(at("2026-01-07 17:00")),
                network_finish: Some(at("2026-01-07 17:00")),
            },
            activities: BTreeMap::from([(A, a), (B, b), (C, c)]),
        }
    }

    #[test]
    fn a_driving_chain_walks_back_to_what_started_it() {
        let result = chain_of_three();
        let chain = result.driving_chain(C, DateField::EarlyStart);
        let steps: Vec<_> = chain.iter().map(|l| (l.activity, l.field)).collect();
        assert_eq!(
            steps,
            [
                (C, DateField::EarlyStart),
                (B, DateField::EarlyStart),
                (A, DateField::EarlyFinish),
                (A, DateField::EarlyStart),
            ]
        );
        assert_eq!(chain[0].date, Some(at("2026-01-07 08:00")));
        assert_eq!(chain[3].provenance, Some(&DateProvenance::DataDate));
    }

    #[test]
    fn a_chain_ends_at_an_activity_outside_the_result() {
        let mut result = chain_of_three();
        let external = ActivityId(99);
        result.activities.get_mut(&A).unwrap().provenance.insert(
            DateField::EarlyStart,
            link(3, external, DateField::EarlyFinish),
        );
        let chain = result.driving_chain(A, DateField::EarlyStart);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[1].activity, external);
        assert_eq!(chain[1].date, None);
        assert_eq!(chain[1].provenance, None);
    }

    #[test]
    fn a_malformed_loop_terminates() {
        let mut result = chain_of_three();
        result
            .activities
            .get_mut(&A)
            .unwrap()
            .provenance
            .insert(DateField::EarlyStart, link(3, C, DateField::EarlyStart));
        let chain = result.driving_chain(C, DateField::EarlyStart);
        assert_eq!(chain.len(), 4);
    }

    #[test]
    fn driver_looks_through_nested_calendar_adjustments() {
        let nested = DateProvenance::CalendarAdjustment {
            unadjusted: at("2026-01-03 08:00"),
            calendar: CalendarId(1),
            cause: Box::new(DateProvenance::CalendarAdjustment {
                unadjusted: at("2026-01-02 08:00"),
                calendar: CalendarId(2),
                cause: Box::new(DateProvenance::ProjectStart),
            }),
        };
        assert_eq!(nested.driver(), &DateProvenance::ProjectStart);
        assert_eq!(DateProvenance::Actual.driver(), &DateProvenance::Actual);
    }

    #[test]
    fn date_reads_each_field_from_its_own_slot() {
        let times = [
            (DateField::EarlyStart, "2026-01-01 08:00"),
            (DateField::EarlyFinish, "2026-01-02 08:00"),
            (DateField::LateStart, "2026-01-03 08:00"),
            (DateField::LateFinish, "2026-01-04 08:00"),
            (DateField::RemainingEarlyStart, "2026-01-05 08:00"),
            (DateField::RemainingEarlyFinish, "2026-01-06 08:00"),
            (DateField::RemainingLateStart, "2026-01-07 08:00"),
            (DateField::RemainingLateFinish, "2026-01-08 08:00"),
        ];
        let mut result = ActivityResult::default();
        let v = &mut result.values;
        for (field, text) in times {
            let slot = match field {
                DateField::EarlyStart => &mut v.early_start,
                DateField::EarlyFinish => &mut v.early_finish,
                DateField::LateStart => &mut v.late_start,
                DateField::LateFinish => &mut v.late_finish,
                DateField::RemainingEarlyStart => &mut v.remaining_early_start,
                DateField::RemainingEarlyFinish => &mut v.remaining_early_finish,
                DateField::RemainingLateStart => &mut v.remaining_late_start,
                DateField::RemainingLateFinish => &mut v.remaining_late_finish,
                DateField::ActualStart | DateField::ActualFinish => continue,
            };
            *slot = Some(at(text));
        }
        for (field, text) in times {
            assert_eq!(result.date(field), Some(at(text)));
        }
    }
}
