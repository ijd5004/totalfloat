//! Expressions on the right of a global change's `--set`, such as
//! `original_duration*0.5` or `early_start+5d`: numbers, spans, the
//! activity's own durations, floats and dates, `+ - * /` and parentheses.
//! An expression has a type, a number, a duration or a date, worked out
//! before any activity is looked at, so a mismatch is refused before
//! anything changes. A duration is in minutes of work. A date plus or minus
//! a duration moves through the activity's calendar's work time, as the
//! engine does; a date less a date is the work time between them.

use totalfloat_calendar::WorkCalendar;
use totalfloat_edit::{Span, duration_lengths};
use totalfloat_model::{Activity, DateTime, Minutes, Schedule};

/// What an expression gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A plain number.
    Number,
    /// Work time.
    Duration,
    /// A date and time.
    Date,
}

impl Kind {
    pub const fn words(self) -> &'static str {
        match self {
            Self::Number => "a number",
            Self::Duration => "a duration",
            Self::Date => "a date",
        }
    }
}

/// An activity's value an expression can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    OriginalDuration,
    RemainingDuration,
    TotalFloat,
    FreeFloat,
    EarlyStart,
    EarlyFinish,
    LateStart,
    LateFinish,
    ActualStart,
    ActualFinish,
    PlannedStart,
    PlannedFinish,
    ConstraintDate,
    DataDate,
}

/// The fields by name, with their kinds.
const FIELDS: [(&str, Field, Kind); 15] = [
    ("duration", Field::OriginalDuration, Kind::Duration),
    ("original_duration", Field::OriginalDuration, Kind::Duration),
    (
        "remaining_duration",
        Field::RemainingDuration,
        Kind::Duration,
    ),
    ("total_float", Field::TotalFloat, Kind::Duration),
    ("free_float", Field::FreeFloat, Kind::Duration),
    ("early_start", Field::EarlyStart, Kind::Date),
    ("early_finish", Field::EarlyFinish, Kind::Date),
    ("late_start", Field::LateStart, Kind::Date),
    ("late_finish", Field::LateFinish, Kind::Date),
    ("actual_start", Field::ActualStart, Kind::Date),
    ("actual_finish", Field::ActualFinish, Kind::Date),
    ("planned_start", Field::PlannedStart, Kind::Date),
    ("planned_finish", Field::PlannedFinish, Kind::Date),
    ("constraint_date", Field::ConstraintDate, Kind::Date),
    ("data_date", Field::DataDate, Kind::Date),
];

/// The names expressions can read, for hints.
pub fn field_names() -> String {
    FIELDS
        .iter()
        .map(|(name, ..)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn name_of(field: Field) -> &'static str {
    FIELDS
        .iter()
        .find(|(_, f, _)| *f == field)
        .map_or("", |(name, ..)| name)
}

/// An arithmetic operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Span(Span),
    Field(Field),
    Neg(Box<Expr>),
    Binary(Box<Expr>, Op, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Span(Span),
    Name(String),
    Op(Op),
    Open,
    Close,
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut k = 0;
    while let Some(&c) = chars.get(k) {
        if c.is_whitespace() {
            k += 1;
            continue;
        }
        let single = match c {
            '+' => Some(Token::Op(Op::Add)),
            '-' => Some(Token::Op(Op::Sub)),
            '*' => Some(Token::Op(Op::Mul)),
            '/' => Some(Token::Op(Op::Div)),
            '(' => Some(Token::Open),
            ')' => Some(Token::Close),
            _ => None,
        };
        if let Some(token) = single {
            out.push(token);
            k += 1;
            continue;
        }
        let start = k;
        if c.is_ascii_digit() || c == '.' {
            while chars
                .get(k)
                .is_some_and(|c| c.is_ascii_digit() || *c == '.')
            {
                k += 1;
            }
            let digits: String = chars[start..k].iter().collect();
            let unit_start = k;
            while chars.get(k).is_some_and(char::is_ascii_alphabetic) {
                k += 1;
            }
            let unit: String = chars[unit_start..k].iter().collect();
            if unit.is_empty() {
                let number = digits
                    .parse::<f64>()
                    .map_err(|_| format!("{digits} isn't a number"))?;
                out.push(Token::Number(number));
            } else {
                let written = format!("{digits}{unit}");
                let span = written
                    .parse::<Span>()
                    .map_err(|_| format!("{written} isn't a span such as 5d, 4h or 2w"))?;
                out.push(Token::Span(span));
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            while chars
                .get(k)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                k += 1;
            }
            out.push(Token::Name(chars[start..k].iter().collect()));
            continue;
        }
        return Err(format!("{c} can't be read here"));
    }
    Ok(out)
}

/// How deeply parentheses and minus signs may nest, well inside the stack.
const MAX_DEPTH: usize = 64;

/// How many numbers, names, operators and parentheses an expression may have.
const MAX_TOKENS: usize = 512;

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut left = self.product()?;
        while let Some(Token::Op(op @ (Op::Add | Op::Sub))) = self.peek() {
            let op = *op;
            self.at += 1;
            left = Expr::Binary(Box::new(left), op, Box::new(self.product()?));
        }
        Ok(left)
    }

    fn product(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        while let Some(Token::Op(op @ (Op::Mul | Op::Div))) = self.peek() {
            let op = *op;
            self.at += 1;
            left = Expr::Binary(Box::new(left), op, Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!("the expression nests more than {MAX_DEPTH} deep"));
        }
        let read = self.atom();
        self.depth -= 1;
        read
    }

    fn atom(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Token::Op(Op::Sub)) {
            self.at += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::Span(s)) => Ok(Expr::Span(s)),
            Some(Token::Name(name)) => FIELDS
                .iter()
                .find(|(n, ..)| *n == name)
                .map(|(_, field, _)| Expr::Field(*field))
                .ok_or_else(|| format!("{name} isn't a field an expression can read")),
            Some(Token::Open) => {
                let inside = self.sum()?;
                if self.next() == Some(Token::Close) {
                    Ok(inside)
                } else {
                    Err("a ( isn't closed".to_owned())
                }
            }
            Some(Token::Close) => Err("a ) has no ( before it".to_owned()),
            Some(Token::Op(_)) => Err("an operator is missing a value before it".to_owned()),
            None => Err("the expression ends too soon".to_owned()),
        }
    }
}

/// Reads an expression.
///
/// # Errors
///
/// What's wrong, in words.
pub fn parse(text: &str) -> Result<Expr, String> {
    let tokens = tokens(text)?;
    // A long chain such as 1+1+1+... nests as deeply as it's long.
    if tokens.len() > MAX_TOKENS {
        return Err(format!("the expression has more than {MAX_TOKENS} parts"));
    }
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
    };
    let expr = parser.sum()?;
    match parser.peek() {
        None => Ok(expr),
        Some(Token::Close) => Err("a ) has no ( before it".to_owned()),
        Some(_) => Err("an operator is missing between two values".to_owned()),
    }
}

/// What an expression gives, worked out from the kinds of what it reads.
///
/// # Errors
///
/// The mismatch, in words.
pub fn kind_of(expr: &Expr) -> Result<Kind, String> {
    use Kind::{Date, Duration, Number};
    Ok(match expr {
        Expr::Number(_) => Number,
        Expr::Span(_) => Duration,
        Expr::Field(field) => FIELDS
            .iter()
            .find(|(_, f, _)| f == field)
            .map_or(Number, |(.., kind)| *kind),
        Expr::Neg(inner) => match kind_of(inner)? {
            Date => return Err("a date can't be negative".to_owned()),
            kind => kind,
        },
        Expr::Binary(left, op, right) => {
            let (a, b) = (kind_of(left)?, kind_of(right)?);
            combined(*op, a, b).ok_or_else(|| {
                let verb = match op {
                    Op::Add => "added to",
                    Op::Sub => "taken from",
                    Op::Mul => "multiplied by",
                    Op::Div => "divided by",
                };
                let (first, second) = if *op == Op::Sub { (b, a) } else { (a, b) };
                format!("{} can't be {verb} {}", first.words(), second.words())
            })?
        }
    })
}

/// What `a op b` gives, if the kinds go together: one arm per rule.
#[allow(
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    reason = "each arm is one rule, read as a table"
)]
const fn combined(op: Op, a: Kind, b: Kind) -> Option<Kind> {
    use Kind::{Date, Duration, Number};
    Some(match (op, a, b) {
        (_, Number, Number) => Number,
        (Op::Add | Op::Sub, Duration, Duration) => Duration,
        (Op::Mul, Duration, Number) => Duration,
        (Op::Mul, Number, Duration) => Duration,
        (Op::Div, Duration, Number) => Duration,
        (Op::Div, Duration, Duration) => Number,
        (Op::Add | Op::Sub, Date, Duration) => Date,
        (Op::Add, Duration, Date) => Date,
        (Op::Sub, Date, Date) => Duration,
        _ => return None,
    })
}

/// Minutes as a number to work with; a schedule's minutes are far inside
/// what an `f64` holds exactly.
#[allow(clippy::cast_precision_loss, reason = "minutes are far below 2^52")]
const fn float(minutes: Minutes) -> f64 {
    minutes.get() as f64
}

/// A value while working an expression out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Number(f64),
    /// Minutes of work.
    Duration(f64),
    Date(DateTime),
}

/// One activity's values, for working expressions out.
pub struct Context<'a> {
    pub schedule: &'a Schedule,
    pub activity: &'a Activity,
    /// The activity's calendar, compiled once for every activity on it.
    calendar: Option<&'a WorkCalendar>,
}

impl<'a> Context<'a> {
    /// The context of `activity` in `schedule`, on its compiled calendar.
    #[must_use]
    pub const fn new(
        schedule: &'a Schedule,
        activity: &'a Activity,
        calendar: Option<&'a WorkCalendar>,
    ) -> Self {
        Self {
            schedule,
            activity,
            calendar,
        }
    }

    const fn calendar(&self) -> Option<&WorkCalendar> {
        self.calendar
    }

    fn field(&self, field: Field) -> Option<Value> {
        let a = self.activity;
        let minutes = |m: Minutes| Value::Duration(float(m));
        let date = |d: Option<DateTime>| d.map(Value::Date);
        match field {
            Field::OriginalDuration => Some(minutes(a.original_duration)),
            Field::RemainingDuration => Some(minutes(a.remaining_duration)),
            Field::TotalFloat => a.stored.total_float.map(minutes),
            Field::FreeFloat => a.stored.free_float.map(minutes),
            Field::EarlyStart => date(a.stored.early_start),
            Field::EarlyFinish => date(a.stored.early_finish),
            Field::LateStart => date(a.stored.late_start),
            Field::LateFinish => date(a.stored.late_finish),
            Field::ActualStart => date(a.actual_start),
            Field::ActualFinish => date(a.actual_finish),
            Field::PlannedStart => date(a.planned_start),
            Field::PlannedFinish => date(a.planned_finish),
            Field::ConstraintDate => date(a.primary_constraint.and_then(|c| c.date)),
            Field::DataDate => date(self.schedule.project.data_date),
        }
    }

    /// Works `expr` out for this activity, which `kind_of` has checked.
    ///
    /// # Errors
    ///
    /// A value the activity doesn't have, a span its calendar can't measure,
    /// a division by zero or a date out of range, in words.
    pub fn value(&self, expr: &Expr) -> Result<Value, String> {
        let code = &self.activity.code;
        Ok(match expr {
            Expr::Number(n) => Value::Number(*n),
            Expr::Span(span) => {
                let lengths = duration_lengths(self.schedule, self.activity);
                let minutes = span
                    .minutes_in(lengths.as_ref())
                    .ok_or_else(|| totalfloat_edit::no_length(code, *span).message)?;
                Value::Duration(float(minutes))
            }
            Expr::Field(field) => self
                .field(*field)
                .ok_or_else(|| format!("{code} has no {}", name_of(*field)))?,
            Expr::Neg(inner) => match self.value(inner)? {
                Value::Number(n) => Value::Number(-n),
                Value::Duration(m) => Value::Duration(-m),
                Value::Date(_) => return Err("a date can't be negative".to_owned()),
            },
            Expr::Binary(left, op, right) => {
                let (a, b) = (self.value(left)?, self.value(right)?);
                self.apply(a, *op, b)?
            }
        })
    }

    #[allow(
        clippy::match_same_arms,
        reason = "each arm is one rule, as in `combined`"
    )]
    fn apply(&self, a: Value, op: Op, b: Value) -> Result<Value, String> {
        use Value::{Date, Duration, Number};
        let divide = |x: f64, y: f64| {
            if y == 0.0 {
                Err("a division by zero".to_owned())
            } else {
                Ok(x / y)
            }
        };
        Ok(match (a, op, b) {
            (Number(x), Op::Add, Number(y)) => Number(x + y),
            (Number(x), Op::Sub, Number(y)) => Number(x - y),
            (Number(x), Op::Mul, Number(y)) => Number(x * y),
            (Number(x), Op::Div, Number(y)) => Number(divide(x, y)?),
            (Duration(x), Op::Add, Duration(y)) => Duration(x + y),
            (Duration(x), Op::Sub, Duration(y)) => Duration(x - y),
            (Duration(x), Op::Mul, Number(y)) | (Number(y), Op::Mul, Duration(x)) => {
                Duration(x * y)
            }
            (Duration(x), Op::Div, Number(y)) => Duration(divide(x, y)?),
            (Duration(x), Op::Div, Duration(y)) => Number(divide(x, y)?),
            (Date(d), Op::Add, Duration(m)) | (Duration(m), Op::Add, Date(d)) => {
                Date(self.moved(d, m)?)
            }
            (Date(d), Op::Sub, Duration(m)) => Date(self.moved(d, -m)?),
            (Date(x), Op::Sub, Date(y)) => {
                let calendar = self.calendar().ok_or_else(|| self.no_calendar())?;
                Duration(float(calendar.work_between(y, x)))
            }
            _ => return Err("the kinds don't match".to_owned()),
        })
    }

    fn no_calendar(&self) -> String {
        format!("{}'s calendar can't be read", self.activity.code)
    }

    /// `date` moved by `minutes` of work on the activity's calendar.
    fn moved(&self, date: DateTime, minutes: f64) -> Result<DateTime, String> {
        let code = &self.activity.code;
        if !minutes.is_finite() || minutes.abs() > 1e12 {
            return Err(format!("{code}'s date would be out of range"));
        }
        let calendar = self.calendar().ok_or_else(|| self.no_calendar())?;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "rounded to whole minutes, far inside i64"
        )]
        let amount = Minutes::new(minutes.round() as i64);
        calendar
            .add_work(date, amount)
            .map_err(|_| format!("{code}'s date would be out of range"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_read_with_their_precedence() {
        let e = parse("original_duration*0.5 + 2d").unwrap();
        assert!(matches!(e, Expr::Binary(_, Op::Add, _)));
        assert_eq!(kind_of(&e), Ok(Kind::Duration));
        assert_eq!(kind_of(&parse("early_start+5d").unwrap()), Ok(Kind::Date));
        assert_eq!(kind_of(&parse("5d+early_start").unwrap()), Ok(Kind::Date));
        assert_eq!(
            kind_of(&parse("late_finish - early_finish").unwrap()),
            Ok(Kind::Duration)
        );
        assert_eq!(
            kind_of(&parse("remaining_duration / duration").unwrap()),
            Ok(Kind::Number)
        );
        assert_eq!(kind_of(&parse("-(2 * (3 + 4))").unwrap()), Ok(Kind::Number));
    }

    #[test]
    fn deep_nesting_is_refused_before_the_stack_runs_out() {
        let deep = format!("{}1d{}", "(".repeat(100), ")".repeat(100));
        assert_eq!(
            parse(&deep).map(|_| ()),
            Err("the expression nests more than 64 deep".to_owned())
        );
        assert!(parse(&"-".repeat(200_000)).is_err());
        let long = vec!["1"; 100_000].join("+");
        assert_eq!(
            parse(&long).map(|_| ()),
            Err("the expression has more than 512 parts".to_owned())
        );
        assert!(parse(&format!("{}1d{}", "(".repeat(60), ")".repeat(60))).is_ok());
        assert!(parse(&vec!["1"; 200].join("+")).is_ok());
    }

    #[test]
    fn mismatches_and_bad_text_are_said_in_words() {
        for (text, said) in [
            ("early_start*2", "a date can't be multiplied by a number"),
            (
                "early_start+early_finish",
                "a date can't be added to a date",
            ),
            ("5d-early_start", "a date can't be taken from a duration"),
            ("2/early_start", "a number can't be divided by a date"),
            ("duration+1", "a duration can't be added to a number"),
            ("-early_start", "a date can't be negative"),
        ] {
            assert_eq!(
                kind_of(&parse(text).unwrap()),
                Err(said.to_owned()),
                "{text}"
            );
        }
        for (text, said) in [
            ("colour*2", "colour isn't a field an expression can read"),
            ("(duration", "a ( isn't closed"),
            ("duration)", "a ) has no ( before it"),
            ("duration 2", "an operator is missing between two values"),
            ("duration*", "the expression ends too soon"),
            ("5x", "5x isn't a span such as 5d, 4h or 2w"),
            ("duration % 2", "% can't be read here"),
        ] {
            assert_eq!(parse(text).map(|_| ()), Err(said.to_owned()), "{text}");
        }
    }
}
