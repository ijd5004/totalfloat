//! Writing a [`Document`] in the canonical layout.

use std::fmt::Write as _;

use crate::parse::{DAYS, FIELD_KINDS, RESOURCE_KINDS, parse_link};
use crate::{
    Activity, ActivityDetail, ActivityKind, CalendarDetail, CodeValue, ConstraintKind, Day, Detail,
    Document, HEADER, Item, LinkType, Predecessor, ProjectDetail, Resource, ResourceDetail,
    RoleDetail, Span, Statement, TypeKind, VERSION,
};

/// Writes a document in the canonical layout: the version line, one space
/// between words, two-space indentation for details, no column alignment,
/// and a blank line before each project, calendar, code type and WBS line,
/// before the first of a run of resources, and between those statements and
/// an activity that follows one.
///
/// Formatting is stable: reading the result and formatting it again gives
/// the same text.
#[must_use]
pub fn format(doc: &Document) -> String {
    let mut out = Out {
        lines: vec![with_comment(
            format!("{HEADER} {VERSION}"),
            doc.comment.as_deref(),
        )],
    };
    for (i, item) in doc.items.iter().enumerate() {
        if i == 0 {
            out.blank();
        }
        match item {
            Item::Blank => out.blank(),
            Item::Comment(comment) => out.lines.push(comment_text(&comment.text)),
            Item::Project(s) => {
                out.blank();
                let head = format!(
                    "project {} {}",
                    ident(&s.head.code.value),
                    quote(&s.head.name.value)
                );
                out.statement(s, head, project_detail);
            }
            Item::Calendar(s) => {
                out.blank();
                let mut head = format!("calendar {}", ident(&s.head.name.value));
                if let Some(copy) = &s.head.copy_of {
                    let _ = write!(head, " copy-of {}", ident(&copy.value));
                }
                out.statement(s, head, calendar_detail);
            }
            Item::CodeType(s) => {
                out.blank();
                let head = format!("code-type {}", ident(&s.head.name.value));
                out.statement(s, head, code_value);
            }
            Item::Resource(s) => {
                // Resources sit together, after a blank line.
                if i > 0 && !matches!(doc.items[i - 1], Item::Resource(_)) {
                    out.blank();
                }
                out.statement(s, resource_line(&s.head), resource_detail);
            }
            Item::Role(s) => {
                // Roles sit together, after a blank line, as resources do.
                if i > 0 && !matches!(doc.items[i - 1], Item::Role(_)) {
                    out.blank();
                }
                let head = format!(
                    "role {} {}",
                    ident(&s.head.code.value),
                    quote(&s.head.name.value)
                );
                out.statement(s, head, role_detail);
            }
            Item::Field(s) => {
                // Fields sit together, after a blank line, as resources do.
                if i > 0 && !matches!(doc.items[i - 1], Item::Field(_)) {
                    out.blank();
                }
                let kind = FIELD_KINDS
                    .iter()
                    .find(|(_, k)| *k == s.head.kind.value)
                    .map_or("text", |(word, _)| *word);
                let head = format!("field {} {kind}", ident(&s.head.name.value));
                out.statement(s, head, |never| match *never {});
            }
            Item::Wbs(s) => {
                out.blank();
                let head = format!(
                    "wbs {} {}",
                    ident(&s.head.path.value),
                    quote(&s.head.name.value)
                );
                out.statement(s, head, |never| match *never {});
            }
            Item::Activity(s) => {
                // Activities on the project's own node follow its other
                // statements after a blank line.
                if i > 0
                    && matches!(
                        doc.items[i - 1],
                        Item::Project(_)
                            | Item::Calendar(_)
                            | Item::CodeType(_)
                            | Item::Field(_)
                            | Item::Resource(_)
                            | Item::Role(_)
                    )
                {
                    out.blank();
                }
                out.statement(s, activity_line(&s.head), activity_detail);
            }
        }
    }
    while out.lines.last().is_some_and(String::is_empty) {
        out.lines.pop();
    }
    let mut text = out.lines.join("\n");
    text.push('\n');
    text
}

struct Out {
    lines: Vec<String>,
}

impl Out {
    fn blank(&mut self) {
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
    }

    fn statement<H, D>(
        &mut self,
        s: &Statement<H, D>,
        head: String,
        detail: impl Fn(&D) -> String,
    ) {
        self.lines.push(with_comment(head, s.comment.as_deref()));
        for line in &s.details {
            self.lines.push(match line {
                Detail::Line { value, comment, .. } => {
                    with_comment(format!("  {}", detail(value)), comment.as_deref())
                }
                Detail::Comment(comment) => format!("  {}", comment_text(&comment.text)),
            });
        }
    }
}

fn comment_text(text: &str) -> String {
    if text.is_empty() {
        "#".to_owned()
    } else {
        format!("# {text}")
    }
}

fn with_comment(line: String, comment: Option<&str>) -> String {
    match comment {
        Some(text) => format!("{line} {}", comment_text(text)),
        None => line,
    }
}

/// A name, always in double quotes.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A code or other identifier: bare when it can be, quoted otherwise.
pub(crate) fn ident(text: &str) -> String {
    let bare = !text.is_empty()
        && !text.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '"' | ',' | '#' | '=' | '\\')
        });
    if bare { text.to_owned() } else { quote(text) }
}

/// A predecessor's code, quoted if it could be read as a link type and lag.
fn predecessor_code(text: &str) -> String {
    if parse_link(text).is_some() {
        quote(text)
    } else {
        ident(text)
    }
}

fn link(predecessor: &Predecessor) -> Option<String> {
    let kind = |link: LinkType| match link {
        LinkType::FS => "fs",
        LinkType::SS => "ss",
        LinkType::FF => "ff",
        LinkType::SF => "sf",
    };
    match (predecessor.link, predecessor.lag) {
        (None | Some(LinkType::FS), None) => None,
        (Some(link), None) => Some(kind(link).to_owned()),
        (link, Some(lag)) => {
            let lag = lag_text(lag);
            Some(format!("{}{lag}", kind(link.unwrap_or(LinkType::FS))))
        }
    }
}

fn lag_text(lag: Span) -> String {
    let text = lag.to_string();
    if text.starts_with('-') {
        text
    } else {
        format!("+{text}")
    }
}

fn resource_line(resource: &Resource) -> String {
    let mut line = format!(
        "resource {} {}",
        ident(&resource.code.value),
        quote(&resource.name.value)
    );
    if let Some(kind) = &resource.kind {
        let word = RESOURCE_KINDS
            .iter()
            .find(|(_, k)| *k == kind.value)
            .map_or("labor", |(word, _)| word);
        let _ = write!(line, " {word}");
    }
    line
}

fn role_detail(detail: &RoleDetail) -> String {
    match detail {
        RoleDetail::Parent(parent) => format!("parent {}", ident(&parent.value)),
        RoleDetail::MaxUnitsPerHour(units) => format!("max-units-per-hour {}", units.value),
    }
}

fn resource_detail(detail: &ResourceDetail) -> String {
    match detail {
        ResourceDetail::Calendar(name) => format!("calendar {}", ident(&name.value)),
        ResourceDetail::UnitsPerHour(units) => format!("units-per-hour {}", units.value),
        ResourceDetail::MaxUnitsPerHour(units) => format!("max-units-per-hour {}", units.value),
    }
}

fn activity_line(activity: &Activity) -> String {
    let keyword = match activity.kind {
        ActivityKind::Activity => "activity",
        ActivityKind::Milestone => "milestone",
        ActivityKind::StartMilestone => "start-milestone",
    };
    let mut line = format!(
        "{keyword} {} {}",
        ident(&activity.code.value),
        quote(&activity.name.value)
    );
    if let Some(duration) = &activity.duration {
        let _ = write!(line, " {}", duration.value);
    }
    if !activity.after.is_empty() {
        let after: Vec<String> = activity
            .after
            .iter()
            .map(|p| match link(p) {
                Some(text) => format!("{} {text}", predecessor_code(&p.code.value)),
                None => predecessor_code(&p.code.value),
            })
            .collect();
        let _ = write!(line, " after {}", after.join(", "));
    }
    line
}

fn project_detail(detail: &ProjectDetail) -> String {
    match detail {
        ProjectDetail::DataDate(date) => format!("data-date {}", date.value),
        ProjectDetail::MustFinish(date) => format!("must-finish {}", date.value),
        ProjectDetail::Calendar(name) => format!("calendar {}", ident(&name.value)),
    }
}

fn day(day: Day) -> &'static str {
    DAYS.iter()
        .find(|(_, d)| *d == day)
        .map_or("mon", |(short, _)| short)
}

fn calendar_detail(detail: &CalendarDetail) -> String {
    match detail {
        CalendarDetail::Week { from, to, shifts } => {
            let days = if from == to {
                day(*from).to_owned()
            } else {
                format!("{}-{}", day(*from), day(*to))
            };
            if shifts.is_empty() {
                format!("week {days} off")
            } else {
                let shifts: Vec<&str> = shifts.iter().map(|s| s.value.as_str()).collect();
                format!("week {days} {}", shifts.join(" "))
            }
        }
        CalendarDetail::Holiday(date) => format!("holiday {}", date.value),
        CalendarDetail::HoursPerDay(hours) => format!("hours-per-day {}", hours.value),
        CalendarDetail::Exception { date, shifts } => {
            let shifts: Vec<&str> = shifts.iter().map(|s| s.value.as_str()).collect();
            format!("exception {} {}", date.value, shifts.join(" "))
        }
    }
}

fn code_value(value: &CodeValue) -> String {
    match &value.description {
        Some(description) => format!(
            "value {} {}",
            ident(&value.value.value),
            quote(&description.value)
        ),
        None => format!("value {}", ident(&value.value.value)),
    }
}

pub(crate) fn constraint_keyword(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::StartOn => "start-on",
        ConstraintKind::StartOnOrAfter => "start-on-or-after",
        ConstraintKind::StartOnOrBefore => "start-on-or-before",
        ConstraintKind::FinishOn => "finish-on",
        ConstraintKind::FinishOnOrAfter => "finish-on-or-after",
        ConstraintKind::FinishOnOrBefore => "finish-on-or-before",
        ConstraintKind::AsLateAsPossible => "as-late-as-possible",
        ConstraintKind::MandatoryStart => "mandatory-start",
        ConstraintKind::MandatoryFinish => "mandatory-finish",
    }
}

fn activity_detail(detail: &ActivityDetail) -> String {
    match detail {
        ActivityDetail::Calendar(name) => format!("calendar {}", ident(&name.value)),
        ActivityDetail::Type(TypeKind::Task) => "type task".to_owned(),
        ActivityDetail::Type(TypeKind::Resource) => "type resource".to_owned(),
        ActivityDetail::Remaining(span) => format!("remaining {}", span.value),
        ActivityDetail::Constraint {
            secondary,
            kind,
            date,
        } => {
            let mut line = String::new();
            if *secondary {
                line.push_str("secondary ");
            }
            line.push_str(constraint_keyword(*kind));
            if let Some(date) = date {
                let _ = write!(line, " {}", date.value);
            }
            line
        }
        ActivityDetail::Code { code_type, value } => {
            let (t, v) = (ident(&code_type.value), ident(&value.value));
            if t == code_type.value && v == value.value {
                format!("code {t}={v}")
            } else {
                format!("code {t} {v}")
            }
        }
        ActivityDetail::Udf { field, value } => {
            let (f, v) = (ident(&field.value), ident(&value.value));
            if f == field.value && v == value.value && !v.contains('=') {
                format!("udf {f}={v}")
            } else {
                format!("udf {f} {v}")
            }
        }
        ActivityDetail::Resource { resource, units } => match units {
            Some(units) => format!("resource {} units {}", ident(&resource.value), units.value),
            None => format!("resource {}", ident(&resource.value)),
        },
        ActivityDetail::Role { role, units } => match units {
            Some(units) => format!("role {} units {}", ident(&role.value), units.value),
            None => format!("role {}", ident(&role.value)),
        },
        ActivityDetail::Expense(x) => {
            let mut line = format!("expense {} {}", quote(&x.item.value), x.budget.value);
            if let Some(actual) = &x.actual {
                let _ = write!(line, " actual {}", actual.value);
            }
            if let Some(remaining) = &x.remaining {
                let _ = write!(line, " remaining {}", remaining.value);
            }
            if let Some(accrual) = &x.accrual {
                let _ = write!(line, " accrual {}", accrual.value.word());
            }
            line
        }
        ActivityDetail::ActualStart(date) => format!("actual-start {}", date.value),
        ActivityDetail::ActualFinish(date) => format!("actual-finish {}", date.value),
        ActivityDetail::Percent(value) => format!("percent {}", value.value),
        ActivityDetail::ExpectedFinish(date) => format!("expected-finish {}", date.value),
    }
}
