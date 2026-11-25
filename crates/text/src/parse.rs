//! Reading a schedule text file into a [`Document`].

use std::str::FromStr;

use crate::diagnostic::closest;
use crate::lex::{self, Token};
use crate::{
    Activity, ActivityDetail, ActivityKind, At, Calendar, CalendarDetail, CodeType, CodeValue,
    Comment, ConstraintKind, Date, DateTime, Day, Detail, Diagnostic, Document, Expense,
    ExpenseAccrual, Field, HEADER, Item, LinkType, Predecessor, Project, ProjectDetail, Resource,
    ResourceDetail, ResourceKind, Role, RoleDetail, Span, Statement, TypeKind, VERSION, Value, Wbs,
    Word,
};

const STATEMENTS: &[&str] = &[
    "project",
    "calendar",
    "code-type",
    "field",
    "resource",
    "role",
    "wbs",
    "activity",
    "milestone",
    "start-milestone",
];

const PROJECT_DETAILS: &[&str] = &["data-date", "must-finish", "calendar"];
const CALENDAR_DETAILS: &[&str] = &["week", "holiday", "exception", "hours-per-day"];
const CODE_TYPE_DETAILS: &[&str] = &["value"];
const RESOURCE_DETAILS: &[&str] = &["calendar", "units-per-hour", "max-units-per-hour"];
const ROLE_DETAILS: &[&str] = &["parent", "max-units-per-hour"];
const EXPENSE_DETAILS: &[&str] = &["actual", "remaining", "accrual"];
/// Expense costs, which a file can hold below zero.
const COSTS: (f64, f64) = (-1e15, 1e15);
/// The data types a `field` line takes, as words.
pub(crate) const FIELD_KINDS: &[(&str, totalfloat_edit::UdfKind)] = &[
    ("text", totalfloat_edit::UdfKind::Text),
    ("number", totalfloat_edit::UdfKind::Number),
    ("integer", totalfloat_edit::UdfKind::Integer),
    ("cost", totalfloat_edit::UdfKind::Cost),
    ("start-date", totalfloat_edit::UdfKind::StartDate),
    ("finish-date", totalfloat_edit::UdfKind::FinishDate),
    ("indicator", totalfloat_edit::UdfKind::Indicator),
];

pub(crate) const RESOURCE_KINDS: &[(&str, ResourceKind)] = &[
    ("labor", ResourceKind::Labor),
    ("nonlabor", ResourceKind::Nonlabor),
    ("material", ResourceKind::Material),
];
const CONSTRAINTS: &[(&str, ConstraintKind)] = &[
    ("start-on", ConstraintKind::StartOn),
    ("start-on-or-after", ConstraintKind::StartOnOrAfter),
    ("start-on-or-before", ConstraintKind::StartOnOrBefore),
    ("finish-on", ConstraintKind::FinishOn),
    ("finish-on-or-after", ConstraintKind::FinishOnOrAfter),
    ("finish-on-or-before", ConstraintKind::FinishOnOrBefore),
    ("as-late-as-possible", ConstraintKind::AsLateAsPossible),
    ("mandatory-start", ConstraintKind::MandatoryStart),
    ("mandatory-finish", ConstraintKind::MandatoryFinish),
];
const ACTIVITY_DETAILS: &[&str] = &[
    "calendar",
    "type",
    "remaining",
    "start-on",
    "start-on-or-after",
    "start-on-or-before",
    "finish-on",
    "finish-on-or-after",
    "finish-on-or-before",
    "as-late-as-possible",
    "mandatory-start",
    "mandatory-finish",
    "secondary",
    "code",
    "udf",
    "resource",
    "role",
    "expense",
    "actual-start",
    "actual-finish",
    "percent",
    "expected-finish",
];

/// Words for values the engine calculates, which the format never holds.
const CALCULATED: &[&str] = &[
    "early-start",
    "early-finish",
    "late-start",
    "late-finish",
    "start",
    "finish",
    "total-float",
    "free-float",
    "float",
    "critical",
];

pub(crate) const DAYS: &[(&str, Day)] = &[
    ("mon", Day::Monday),
    ("tue", Day::Tuesday),
    ("wed", Day::Wednesday),
    ("thu", Day::Thursday),
    ("fri", Day::Friday),
    ("sat", Day::Saturday),
    ("sun", Day::Sunday),
];

/// Reads a schedule text file. `file` is the name used in diagnostics.
///
/// # Errors
///
/// Every problem found, in line order. Reading recovers at the next statement
/// line, so one mistake doesn't hide the rest.
pub fn parse(source: &str, file: &str) -> Result<Document, Vec<Diagnostic>> {
    let mut parser = Parser {
        file,
        problems: Vec::new(),
        doc: Document::default(),
        current: Current::None,
        blank: false,
    };
    let mut lines = source.lines().enumerate().map(|(i, text)| (i + 1, text));
    if !parser.header(&mut lines) {
        return Err(parser.problems);
    }
    for (number, text) in lines {
        match lex::line(text, number, file) {
            Ok(line) => parser.line(line),
            Err(problem) => {
                parser.problems.push(problem);
                if !text.starts_with(char::is_whitespace) {
                    parser.current = Current::Broken;
                }
            }
        }
    }
    if parser.problems.is_empty() {
        Ok(parser.doc)
    } else {
        Err(parser.problems)
    }
}

/// The statement that indented lines belong to.
enum Current {
    None,
    Open,
    /// A statement line that couldn't be read: its details are skipped.
    Broken,
}

struct Parser<'f> {
    file: &'f str,
    problems: Vec<Diagnostic>,
    doc: Document,
    current: Current,
    blank: bool,
}

impl Parser<'_> {
    fn problem(&self, code: &'static str, at: At, message: String, hint: &str) -> Diagnostic {
        Diagnostic {
            code,
            message,
            hint: hint.to_owned(),
            file: self.file.to_owned(),
            at,
        }
    }

    /// Reads the version line. False if the file can't be read at all.
    fn header<'s>(&mut self, lines: &mut impl Iterator<Item = (usize, &'s str)>) -> bool {
        let first = lines.find(|(_, text)| !text.trim().is_empty());
        let (number, text) = first.unwrap_or((1, ""));
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let (code, comment) = match text.split_once('#') {
            Some((code, comment)) => (code, Some(comment.trim().to_owned())),
            None => (text, None),
        };
        let words: Vec<&str> = code.split_whitespace().collect();
        let at = At {
            line: number,
            column: text.len() - text.trim_start().len() + 1,
            length: text.trim().chars().count().max(1),
        };
        let wanted = format!("{HEADER} {VERSION}");
        match words.as_slice() {
            [HEADER, version] if *version == VERSION.to_string() => {
                self.doc.comment = comment;
                true
            }
            [HEADER, version] => {
                let problem = self.problem(
                    "wrong-version",
                    at,
                    format!("this file is version {version}, and this TotalFloat reads version {VERSION}"),
                    "use a TotalFloat that reads this version, or write the file as version 1",
                );
                self.problems.push(problem);
                false
            }
            _ => {
                let problem = self.problem(
                    "missing-version",
                    at,
                    format!("a schedule text file starts with `{wanted}`"),
                    &format!("put `{wanted}` on the first line"),
                );
                self.problems.push(problem);
                false
            }
        }
    }

    fn line(&mut self, line: lex::Line) {
        if line.tokens.is_empty() {
            match line.comment {
                None => self.blank = !self.doc.items.is_empty(),
                Some((text, at)) => self.comment_line(line.indented, Comment { text, at }),
            }
            return;
        }
        let comment = line.comment.map(|(text, _)| text);
        if line.indented {
            match self.current {
                Current::Open => {
                    if let Err(problem) = self.detail(&line.tokens, comment) {
                        self.problems.push(problem);
                    }
                }
                Current::Broken => {}
                Current::None => {
                    let problem = self.problem(
                        "detail-without-statement",
                        line.tokens[0].at(),
                        "this indented line isn't under a statement".to_owned(),
                        "indented lines are details of the statement above them; start a statement such as `activity` at the beginning of a line",
                    );
                    self.problems.push(problem);
                }
            }
            return;
        }
        self.flush_blank();
        match self.statement(&line.tokens, comment) {
            Ok(item) => {
                self.doc.items.push(item);
                self.current = Current::Open;
            }
            Err(problem) => {
                self.problems.push(problem);
                self.current = Current::Broken;
            }
        }
    }

    fn flush_blank(&mut self) {
        if std::mem::take(&mut self.blank) {
            self.doc.items.push(Item::Blank);
        }
    }

    fn comment_line(&mut self, indented: bool, comment: Comment) {
        if indented && matches!(self.current, Current::Broken) {
            return;
        }
        if indented
            && matches!(self.current, Current::Open)
            && let Some(item) = self.doc.items.last_mut()
        {
            push_detail_comment(item, comment);
            return;
        }
        self.flush_blank();
        self.doc.items.push(Item::Comment(comment));
        self.current = Current::None;
    }

    fn statement(&self, tokens: &[Token], comment: Option<String>) -> Result<Item, Diagnostic> {
        let mut words = Words::new(self, tokens);
        let (keyword, at) = words.keyword(STATEMENTS, "statement")?;
        let item = match keyword {
            "project" => {
                let code = words.word("the project code")?;
                let name = words.name("the project name")?;
                Item::Project(statement(Project { code, name }, at, comment))
            }
            "calendar" => {
                let name = words.word("the calendar name")?;
                let copy_of = if words.at_end() {
                    None
                } else {
                    words.literal("copy-of")?;
                    Some(words.word("the calendar to copy")?)
                };
                Item::Calendar(statement(Calendar { name, copy_of }, at, comment))
            }
            "code-type" => {
                let name = words.word("the code type's name")?;
                Item::CodeType(statement(CodeType { name }, at, comment))
            }
            "field" => {
                let name = words.word("the field's name")?;
                let names: Vec<&str> = FIELD_KINDS.iter().map(|(k, _)| *k).collect();
                let (word, kind_at) = words.keyword(&names, "field type")?;
                let kind = FIELD_KINDS
                    .iter()
                    .find(|(k, _)| *k == word)
                    .map(|(_, kind)| Value {
                        value: *kind,
                        at: kind_at,
                    })
                    .ok_or_else(|| {
                        words.problem(
                            "bad-field-type",
                            kind_at,
                            format!("{word} isn't a field type"),
                            "write text, number, integer, cost, start-date, finish-date or indicator",
                        )
                    })?;
                Item::Field(statement(Field { name, kind }, at, comment))
            }
            "resource" => {
                let code = words.word("the resource's short name")?;
                let name = words.name("the resource's name")?;
                let kind = if words.at_end() {
                    None
                } else {
                    let names: Vec<&str> = RESOURCE_KINDS.iter().map(|(k, _)| *k).collect();
                    let (word, at) = words.keyword(&names, "resource type")?;
                    RESOURCE_KINDS
                        .iter()
                        .find(|(k, _)| *k == word)
                        .map(|(_, kind)| Value { value: *kind, at })
                };
                Item::Resource(statement(Resource { code, name, kind }, at, comment))
            }
            "role" => {
                let code = words.word("the role's short name")?;
                let name = words.name("the role's name")?;
                Item::Role(statement(Role { code, name }, at, comment))
            }
            "wbs" => {
                let path = words.word("the WBS path, such as PLANT.CIV")?;
                let name = words.name("the WBS name")?;
                Item::Wbs(statement(Wbs { path, name }, at, comment))
            }
            _ => {
                let kind = match keyword {
                    "activity" => ActivityKind::Activity,
                    "milestone" => ActivityKind::Milestone,
                    _ => ActivityKind::StartMilestone,
                };
                Item::Activity(statement(words.activity(kind)?, at, comment))
            }
        };
        words.end()?;
        Ok(item)
    }

    fn detail(&mut self, tokens: &[Token], comment: Option<String>) -> Result<(), Diagnostic> {
        let mut words = Words::new(self, tokens);
        let Some(item) = self.doc.items.last() else {
            return Ok(());
        };
        let pushed = match item {
            Item::Project(_) => Pushed::Project(words.project_detail()?),
            Item::Calendar(_) => Pushed::Calendar(words.calendar_detail()?),
            Item::CodeType(_) => Pushed::CodeValue(words.code_value()?),
            Item::Resource(_) => Pushed::Resource(words.resource_detail()?),
            Item::Role(_) => Pushed::Role(words.role_detail()?),
            Item::Activity(_) => Pushed::Activity(words.activity_detail()?),
            Item::Wbs(_) => {
                return Err(self.problem(
                    "unexpected-detail",
                    tokens[0].at(),
                    "a wbs line takes no details".to_owned(),
                    "put activities on lines of their own, at the beginning of the line",
                ));
            }
            Item::Field(_) => {
                return Err(self.problem(
                    "unexpected-detail",
                    tokens[0].at(),
                    "a field line takes no details".to_owned(),
                    "give values on activities, as udf NAME=VALUE",
                ));
            }
            Item::Blank | Item::Comment(_) => return Ok(()),
        };
        words.end()?;
        let at = tokens[0].at();
        if let Some(item) = self.doc.items.last_mut() {
            pushed.push(item, at, comment);
        }
        Ok(())
    }
}

fn statement<H, D>(head: H, at: At, comment: Option<String>) -> Statement<H, D> {
    Statement {
        head,
        at,
        comment,
        details: Vec::new(),
    }
}

fn push_detail_comment(item: &mut Item, comment: Comment) {
    match item {
        Item::Project(s) => s.details.push(Detail::Comment(comment)),
        Item::Calendar(s) => s.details.push(Detail::Comment(comment)),
        Item::CodeType(s) => s.details.push(Detail::Comment(comment)),
        Item::Resource(s) => s.details.push(Detail::Comment(comment)),
        Item::Role(s) => s.details.push(Detail::Comment(comment)),
        Item::Wbs(s) => s.details.push(Detail::Comment(comment)),
        Item::Field(s) => s.details.push(Detail::Comment(comment)),
        Item::Activity(s) => s.details.push(Detail::Comment(comment)),
        Item::Blank | Item::Comment(_) => {}
    }
}

/// A detail read, waiting to go under its statement.
enum Pushed {
    Project(ProjectDetail),
    Calendar(CalendarDetail),
    CodeValue(CodeValue),
    Resource(ResourceDetail),
    Role(RoleDetail),
    Activity(ActivityDetail),
}

impl Pushed {
    fn push(self, item: &mut Item, at: At, comment: Option<String>) {
        fn line<D>(value: D, at: At, comment: Option<String>) -> Detail<D> {
            Detail::Line { value, at, comment }
        }
        match (self, item) {
            (Self::Project(d), Item::Project(s)) => s.details.push(line(d, at, comment)),
            (Self::Calendar(d), Item::Calendar(s)) => s.details.push(line(d, at, comment)),
            (Self::CodeValue(d), Item::CodeType(s)) => s.details.push(line(d, at, comment)),
            (Self::Resource(d), Item::Resource(s)) => s.details.push(line(d, at, comment)),
            (Self::Role(d), Item::Role(s)) => s.details.push(line(d, at, comment)),
            (Self::Activity(d), Item::Activity(s)) => s.details.push(line(d, at, comment)),
            _ => {}
        }
    }
}

/// The words of one line, read left to right.
struct Words<'p, 't> {
    parser: &'p Parser<'p>,
    tokens: &'t [Token],
    next: usize,
}

impl<'p, 't> Words<'p, 't> {
    fn new(parser: &'p Parser<'p>, tokens: &'t [Token]) -> Self {
        Self {
            parser,
            tokens,
            next: 0,
        }
    }

    fn problem(&self, code: &'static str, at: At, message: String, hint: &str) -> Diagnostic {
        self.parser.problem(code, at, message, hint)
    }

    fn at_end(&self) -> bool {
        self.next >= self.tokens.len()
    }

    /// Just past the last word, for something missing at the end.
    fn end_at(&self) -> At {
        let last = self.tokens.last().map(Token::at).unwrap_or_default();
        At {
            line: last.line,
            column: last.column + last.length,
            length: 1,
        }
    }

    fn peek(&self) -> Option<&'t Token> {
        self.tokens.get(self.next)
    }

    /// The next token, if it's a bare word.
    fn peek_bare(&self) -> Option<(&'t str, At)> {
        match self.peek()? {
            Token::Word {
                text,
                quoted: false,
                at,
            } => Some((text.as_str(), *at)),
            _ => None,
        }
    }

    fn missing(&self, what: &str) -> Diagnostic {
        self.problem(
            "missing-value",
            self.end_at(),
            format!("{what} is missing"),
            &format!("add {what} here"),
        )
    }

    fn end(&self) -> Result<(), Diagnostic> {
        match self.peek() {
            None => Ok(()),
            Some(Token::Comma(at)) => Err(self.problem(
                "unexpected-word",
                *at,
                "a comma only separates predecessors after `after`".to_owned(),
                "take the comma out",
            )),
            Some(Token::Word { text, at, .. }) => Err(self.problem(
                "unexpected-word",
                *at,
                format!("`{text}` wasn't expected here"),
                "take it out, or put it on a line of its own if it's a detail; names with spaces go in double quotes",
            )),
        }
    }

    /// Any word, bare or quoted.
    fn word(&mut self, what: &str) -> Result<Word, Diagnostic> {
        match self.peek() {
            Some(Token::Word { text, at, .. }) => {
                self.next += 1;
                Ok(Value {
                    value: text.clone(),
                    at: *at,
                })
            }
            Some(Token::Comma(at)) => Err(self.problem(
                "unexpected-word",
                *at,
                format!("expected {what}, not a comma"),
                "take the comma out",
            )),
            None => Err(self.missing(what)),
        }
    }

    /// A quoted name.
    fn name(&mut self, what: &str) -> Result<Word, Diagnostic> {
        match self.peek() {
            Some(Token::Word {
                quoted: false,
                text,
                at,
            }) => Err(self.problem(
                "expected-name",
                *at,
                format!("{what} goes in double quotes"),
                &format!("write it as \"{text}\""),
            )),
            _ => self.word(what),
        }
    }

    /// A specific bare word.
    fn literal(&mut self, wanted: &str) -> Result<(), Diagnostic> {
        match self.peek_bare() {
            Some((text, _)) if text == wanted => {
                self.next += 1;
                Ok(())
            }
            Some((text, at)) => Err(self.problem(
                "unexpected-word",
                at,
                format!("expected `{wanted}`, not `{text}`"),
                &format!("write `{wanted}` here, or take the rest of the line out"),
            )),
            None => match self.peek() {
                Some(token) => Err(self.problem(
                    "unexpected-word",
                    token.at(),
                    format!("expected `{wanted}` here"),
                    &format!("write `{wanted}` here"),
                )),
                None => Err(self.missing(&format!("`{wanted}`"))),
            },
        }
    }

    /// One of `known`, as a bare word at the start of a line.
    fn keyword(
        &mut self,
        known: &[&'static str],
        what: &str,
    ) -> Result<(&'static str, At), Diagnostic> {
        let token = self.peek().ok_or_else(|| self.missing(what))?;
        let at = token.at();
        let (text, quoted) = match token {
            Token::Word { text, quoted, .. } => (text.as_str(), *quoted),
            Token::Comma(_) => (",", false),
        };
        if !quoted && let Some(found) = known.iter().find(|k| **k == text) {
            self.next += 1;
            return Ok((*found, at));
        }
        if CALCULATED.contains(&text) {
            return Err(self.problem(
                "calculated-value",
                at,
                format!("`{text}` is calculated by the engine, so it can't be written here"),
                "pin a date with a constraint such as start-on-or-after instead",
            ));
        }
        let hint = closest(text, known).map_or_else(
            || format!("a {what} starts with one of: {}", known.join(", ")),
            |near| format!("did you mean `{near}`?"),
        );
        let (code, kind) = if what == "statement" {
            ("unknown-keyword", "statement")
        } else {
            ("unknown-detail", what)
        };
        Err(self.problem(code, at, format!("`{text}` isn't a {kind}"), &hint))
    }

    fn span(&mut self, what: &str) -> Result<Value<Span>, Diagnostic> {
        let word = self.word(what)?;
        parse_span(&word.value)
            .map(|value| Value { value, at: word.at })
            .map_err(|message| self.bad_span(word.at, message))
    }

    /// A span that can't be negative.
    fn duration(&mut self, what: &str) -> Result<Value<Span>, Diagnostic> {
        let span = self.span(what)?;
        if span.value.to_string().starts_with('-') {
            return Err(self.problem(
                "bad-span",
                span.at,
                format!("{what} can't be negative"),
                "give a duration of 0 or more; a negative span is only for a lead on a link",
            ));
        }
        Ok(span)
    }

    fn bad_span(&self, at: At, message: String) -> Diagnostic {
        self.problem(
            "bad-span",
            at,
            message,
            "write a span as 5d or 40h, with - for a lead",
        )
    }

    fn number(&mut self, what: &str, range: (f64, f64)) -> Result<Value<f64>, Diagnostic> {
        let word = self.word(what)?;
        match word.value.parse::<f64>() {
            Ok(value) if value.is_finite() && value >= range.0 && value <= range.1 => {
                Ok(Value { value, at: word.at })
            }
            _ => Err(self.problem(
                "bad-number",
                word.at,
                format!(
                    "{} isn't a number from {} to {}",
                    word.value, range.0, range.1
                ),
                &format!("give {what} as a plain number, such as 2 or 0.5"),
            )),
        }
    }

    /// `YYYY-MM-DD HH:MM`, as two words.
    fn datetime(&mut self, what: &str) -> Result<Value<DateTime>, Diagnostic> {
        let date = self.word(what)?;
        let time = match self.peek_bare() {
            Some((text, at)) if text.contains(':') => {
                self.next += 1;
                Some((text, at))
            }
            _ => None,
        };
        let at = time.map_or(date.at, |(_, t)| At {
            length: t.column + t.length - date.at.column,
            ..date.at
        });
        let hint = "write the date and time as 2026-03-02 08:00";
        let Some((time, _)) = time else {
            let message = if Date::from_str(&date.value).is_ok() {
                format!("{} needs a time as well", date.value)
            } else {
                format!("{} isn't a date and time", date.value)
            };
            return Err(self.problem("bad-date", at, message, hint));
        };
        let text = format!("{} {time}", date.value);
        DateTime::from_str(&text)
            .map(|value| Value { value, at })
            .map_err(|_| {
                self.problem(
                    "bad-date",
                    at,
                    format!("{text} isn't a real date and time"),
                    hint,
                )
            })
    }

    fn date(&mut self, what: &str) -> Result<Value<Date>, Diagnostic> {
        let word = self.word(what)?;
        Date::from_str(&word.value)
            .map(|value| Value { value, at: word.at })
            .map_err(|_| {
                self.problem(
                    "bad-date",
                    word.at,
                    format!("{} isn't a date", word.value),
                    "write the date as 2026-05-25",
                )
            })
    }

    fn activity(&mut self, kind: ActivityKind) -> Result<Activity, Diagnostic> {
        let code = self.word("the activity ID")?;
        let name = self.name("the activity name")?;
        let mut duration = None;
        if let Some((text, at)) = self.peek_bare()
            && text != "after"
        {
            if kind != ActivityKind::Activity {
                return Err(self.problem(
                    "milestone-duration",
                    at,
                    "a milestone has no duration".to_owned(),
                    "take the duration out, or make it an `activity`",
                ));
            }
            duration = Some(self.duration("the duration")?);
        }
        let mut after = Vec::new();
        if !self.at_end() {
            self.literal("after")?;
            after = self.predecessors()?;
        }
        Ok(Activity {
            kind,
            code,
            name,
            duration,
            after,
        })
    }

    fn predecessors(&mut self) -> Result<Vec<Predecessor>, Diagnostic> {
        let mut after = Vec::new();
        loop {
            let code = self.word("a predecessor's activity ID")?;
            let mut predecessor = Predecessor {
                code,
                link: None,
                lag: None,
                link_at: None,
            };
            if let Some((text, at)) = self.peek_bare() {
                let (link, lag) = parse_link(text).ok_or_else(|| {
                    self.problem(
                        "bad-link",
                        at,
                        format!("`{text}` isn't a link type and lag"),
                        "write the type and lag as ss, ff+2d or fs-1d, and separate predecessors with commas",
                    )
                })?;
                let lag = lag
                    .map(|text| parse_span(text).map_err(|message| self.bad_span(at, message)))
                    .transpose()?;
                predecessor.link = link;
                predecessor.lag = lag;
                predecessor.link_at = Some(at);
                self.next += 1;
            }
            after.push(predecessor);
            match self.peek() {
                Some(Token::Comma(_)) => self.next += 1,
                _ => return Ok(after),
            }
        }
    }

    fn project_detail(&mut self) -> Result<ProjectDetail, Diagnostic> {
        let (keyword, _) = self.keyword(PROJECT_DETAILS, "project detail")?;
        Ok(match keyword {
            "data-date" => ProjectDetail::DataDate(self.datetime("the data date")?),
            "must-finish" => ProjectDetail::MustFinish(self.datetime("the must-finish date")?),
            _ => ProjectDetail::Calendar(self.word("the calendar name")?),
        })
    }

    fn calendar_detail(&mut self) -> Result<CalendarDetail, Diagnostic> {
        let (keyword, _) = self.keyword(CALENDAR_DETAILS, "calendar detail")?;
        Ok(match keyword {
            "week" => {
                let (from, to) = self.days()?;
                let shifts = if self.peek_bare().is_some_and(|(t, _)| t == "off") {
                    self.next += 1;
                    Vec::new()
                } else {
                    let shifts = self.shifts()?;
                    if shifts.is_empty() {
                        return Err(self.missing("the shifts (such as 08:00-12:00), or off,"));
                    }
                    shifts
                };
                CalendarDetail::Week { from, to, shifts }
            }
            "holiday" => CalendarDetail::Holiday(self.date("the holiday's date")?),
            "hours-per-day" => {
                CalendarDetail::HoursPerDay(self.number("the hours per day", (0.0, 24.0))?)
            }
            _ => {
                let date = self.date("the exception's date")?;
                let shifts = self.shifts()?;
                if shifts.is_empty() {
                    return Err(self.missing("the day's shifts, such as 08:00-12:00,"));
                }
                CalendarDetail::Exception { date, shifts }
            }
        })
    }

    fn days(&mut self) -> Result<(Day, Day), Diagnostic> {
        let word = self.word("the days, such as mon-fri")?;
        let bad = || {
            self.problem(
                "bad-day",
                word.at,
                format!("`{}` isn't a day or a range of days", word.value),
                "write a day as mon to sun, or a range in week order such as mon-fri",
            )
        };
        let index = |text: &str| {
            let text = text.to_lowercase();
            DAYS.iter()
                .position(|(short, _)| text == *short || text == full_day(short))
        };
        let (first, last) = word
            .value
            .split_once('-')
            .unwrap_or((&word.value, &word.value));
        let (Some(first), Some(last)) = (index(first), index(last)) else {
            return Err(bad());
        };
        if first > last {
            return Err(bad());
        }
        Ok((DAYS[first].1, DAYS[last].1))
    }

    fn shifts(&mut self) -> Result<Vec<Value<String>>, Diagnostic> {
        let mut shifts = Vec::new();
        while let Some((text, at)) = self.peek_bare() {
            if !is_shift(text) {
                return Err(self.problem(
                    "bad-shift",
                    at,
                    format!("`{text}` isn't a shift"),
                    "write a shift as 08:00-12:00, with the end after the start",
                ));
            }
            shifts.push(Value {
                value: text.to_owned(),
                at,
            });
            self.next += 1;
        }
        Ok(shifts)
    }

    fn code_value(&mut self) -> Result<CodeValue, Diagnostic> {
        self.keyword(CODE_TYPE_DETAILS, "code type detail")?;
        let value = self.word("the code value")?;
        let description = if self.at_end() {
            None
        } else {
            Some(self.name("the value's description")?)
        };
        Ok(CodeValue { value, description })
    }

    fn resource_detail(&mut self) -> Result<ResourceDetail, Diagnostic> {
        let (keyword, _) = self.keyword(RESOURCE_DETAILS, "resource detail")?;
        Ok(match keyword {
            "calendar" => ResourceDetail::Calendar(self.word("the calendar name")?),
            "max-units-per-hour" => ResourceDetail::MaxUnitsPerHour(
                self.number("the most units per hour", (0.0, f64::MAX))?,
            ),
            _ => ResourceDetail::UnitsPerHour(self.number("the units per hour", (0.0, f64::MAX))?),
        })
    }

    fn role_detail(&mut self) -> Result<RoleDetail, Diagnostic> {
        let (keyword, _) = self.keyword(ROLE_DETAILS, "role detail")?;
        Ok(match keyword {
            "max-units-per-hour" => RoleDetail::MaxUnitsPerHour(
                self.number("the most units per hour", (0.0, f64::MAX))?,
            ),
            _ => RoleDetail::Parent(self.word("the parent role's short name")?),
        })
    }

    /// The rest of an `expense` line.
    fn expense(&mut self) -> Result<Expense, Diagnostic> {
        let item = self.name("the expense item")?;
        let budget = self.number("the budgeted cost", COSTS)?;
        let mut expense = Expense {
            item,
            budget,
            actual: None,
            remaining: None,
            accrual: None,
        };
        while !self.at_end() {
            let (keyword, _) = self.keyword(EXPENSE_DETAILS, "expense detail")?;
            match keyword {
                "actual" => expense.actual = Some(self.number("the actual cost", COSTS)?),
                "remaining" => {
                    expense.remaining = Some(self.number("the remaining cost", COSTS)?);
                }
                _ => {
                    let word = self.word("start, end or uniform")?;
                    let Some(value) = ExpenseAccrual::from_word(&word.value) else {
                        return Err(self.problem(
                            "bad-accrual",
                            word.at,
                            format!("`{}` isn't an accrual", word.value),
                            "write accrual start, end or uniform",
                        ));
                    };
                    expense.accrual = Some(Value { value, at: word.at });
                }
            }
        }
        Ok(expense)
    }

    fn activity_detail(&mut self) -> Result<ActivityDetail, Diagnostic> {
        let (keyword, _) = self.keyword(ACTIVITY_DETAILS, "activity detail")?;
        Ok(match keyword {
            "calendar" => ActivityDetail::Calendar(self.word("the calendar name")?),
            "type" => {
                let word = self.word("task or resource")?;
                ActivityDetail::Type(match word.value.as_str() {
                    "task" => TypeKind::Task,
                    "resource" => TypeKind::Resource,
                    other => {
                        return Err(self.problem(
                            "bad-type",
                            word.at,
                            format!("`{other}` isn't an activity type"),
                            "write type task or type resource; milestones have keywords of their own",
                        ));
                    }
                })
            }
            "remaining" => ActivityDetail::Remaining(self.duration("the remaining duration")?),
            "secondary" => {
                let (keyword, _) = self.keyword(
                    &CONSTRAINTS.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                    "constraint",
                )?;
                self.constraint(keyword, true)?
            }
            "code" => self.code()?,
            "udf" => {
                let (field, value) = self.named_pair(("bad-udf", "a field"), "udf Risk=2")?;
                ActivityDetail::Udf { field, value }
            }
            "resource" => {
                let resource = self.word("the resource's short name")?;
                let units = if self.at_end() {
                    None
                } else {
                    self.literal("units")?;
                    Some(self.number("the units", (0.0, f64::MAX))?)
                };
                ActivityDetail::Resource { resource, units }
            }
            "role" => {
                let role = self.word("the role's short name")?;
                let units = if self.at_end() {
                    None
                } else {
                    self.literal("units")?;
                    Some(self.number("the units", (0.0, f64::MAX))?)
                };
                ActivityDetail::Role { role, units }
            }
            "expense" => ActivityDetail::Expense(self.expense()?),
            "actual-start" => ActivityDetail::ActualStart(self.datetime("the actual start")?),
            "actual-finish" => ActivityDetail::ActualFinish(self.datetime("the actual finish")?),
            "percent" => {
                ActivityDetail::Percent(self.number("the percent complete", (0.0, 100.0))?)
            }
            "expected-finish" => {
                ActivityDetail::ExpectedFinish(self.datetime("the expected finish")?)
            }
            constraint => self.constraint(constraint, false)?,
        })
    }

    fn constraint(&mut self, keyword: &str, secondary: bool) -> Result<ActivityDetail, Diagnostic> {
        let kind = CONSTRAINTS
            .iter()
            .find(|(k, _)| *k == keyword)
            .map_or(ConstraintKind::AsLateAsPossible, |(_, kind)| *kind);
        // A missing date is the edit's to refuse, so it's reported with the
        // file's other problems rather than stopping the read.
        let date = if self.at_end() {
            None
        } else {
            Some(self.datetime("the constraint's date")?)
        };
        Ok(ActivityDetail::Constraint {
            secondary,
            kind,
            date,
        })
    }

    fn code(&mut self) -> Result<ActivityDetail, Diagnostic> {
        let (code_type, value) = self.named_pair(("bad-code", "a code type"), "code Phase=CON")?;
        Ok(ActivityDetail::Code { code_type, value })
    }

    /// `NAME=VALUE`, or the two as separate words: a code or field and its
    /// value. `what` names the problem's code and the first word, `example`
    /// the form to write.
    fn named_pair(
        &mut self,
        (code, what): (&'static str, &str),
        example: &str,
    ) -> Result<(Word, Word), Diagnostic> {
        if let Some((text, at)) = self.peek_bare()
            && let Some((name, value)) = text.split_once('=')
        {
            if name.is_empty() || value.is_empty() {
                return Err(self.problem(
                    code,
                    at,
                    format!("`{text}` needs {what} and a value"),
                    &format!("write it as {example}"),
                ));
            }
            self.next += 1;
            let left = At {
                length: name.chars().count(),
                ..at
            };
            let right = At {
                column: at.column + left.length + 1,
                length: value.chars().count(),
                ..at
            };
            return Ok((
                Value {
                    value: name.to_owned(),
                    at: left,
                },
                Value {
                    value: value.to_owned(),
                    at: right,
                },
            ));
        }
        let name = self.word(&format!("the name of {what}"))?;
        let value = self.word("the value")?;
        Ok((name, value))
    }
}

fn full_day(short: &str) -> &'static str {
    match short {
        "mon" => "monday",
        "tue" => "tuesday",
        "wed" => "wednesday",
        "thu" => "thursday",
        "fri" => "friday",
        "sat" => "saturday",
        _ => "sunday",
    }
}

fn parse_span(text: &str) -> Result<Span, String> {
    // Schedule text takes hours and days; weeks, months and years are for edits.
    Span::from_str(text)
        .ok()
        .filter(|span| span.in_hours_or_days())
        .ok_or_else(|| format!("`{text}` isn't a span"))
}

/// A link token: `ss`, `ff+2d`, `fs-1d` or `+2d`. The lag is returned as text
/// to parse, with its sign.
pub(crate) fn parse_link(text: &str) -> Option<(Option<LinkType>, Option<&str>)> {
    let lower = text.get(..2).map(str::to_lowercase);
    let link = match lower.as_deref() {
        Some("fs") => Some(LinkType::FS),
        Some("ss") => Some(LinkType::SS),
        Some("ff") => Some(LinkType::FF),
        Some("sf") => Some(LinkType::SF),
        _ => None,
    };
    let rest = if link.is_some() { &text[2..] } else { text };
    match rest.chars().next() {
        None if link.is_some() => Some((link, None)),
        Some('+') => Some((link, Some(&rest[1..]))),
        Some('-') => Some((link, Some(rest))),
        _ => None,
    }
}

/// `HH:MM-HH:MM`, with the end after the start; `24:00` ends a day.
pub(crate) fn is_shift(text: &str) -> bool {
    shift_minutes(text).is_some()
}

/// A shift's length in minutes, if it's a shift such as `08:00-12:00`.
pub(crate) fn shift_minutes(text: &str) -> Option<u32> {
    let minutes = |t: &str| -> Option<u32> {
        let (h, m) = t.split_once(':')?;
        let digits = |t: &str| t.len() == 2 && t.bytes().all(|b| b.is_ascii_digit());
        if !digits(h) || !digits(m) {
            return None;
        }
        let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
        (m < 60 && (h < 24 || (h == 24 && m == 0))).then_some(h * 60 + m)
    };
    text.split_once('-')
        .and_then(|(a, b)| Some((minutes(a)?, minutes(b)?)))
        .and_then(|(a, b)| (a < b).then(|| b - a))
}
