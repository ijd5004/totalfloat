//! Who is recording a changeset, and when. Read here, in one place, and passed
//! to the store as inputs.

use totalfloat_store::AuthorKind;

use crate::error::CliError;

/// The time now, in UTC as RFC 3339, or `TOTALFLOAT_NOW` when it's set, so
/// tests and reproducible runs can fix it.
///
/// # Errors
///
/// `invalid-date` if `TOTALFLOAT_NOW` isn't `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now() -> Result<String, CliError> {
    if let Ok(fixed) = std::env::var("TOTALFLOAT_NOW") {
        if !is_utc_time(&fixed) {
            return Err(CliError::new(
                "invalid-date",
                format!("TOTALFLOAT_NOW is {fixed}"),
                "give it as YYYY-MM-DDTHH:MM:SSZ, in UTC, or leave it unset",
            ));
        }
        return Ok(fixed);
    }
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Ok(rfc3339(seconds))
}

/// True for `YYYY-MM-DDTHH:MM:SSZ` with a real date and time.
fn is_utc_time(text: &str) -> bool {
    let shape = text.len() == 20
        && text.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            10 => b == b'T',
            13 | 16 => b == b':',
            19 => b == b'Z',
            _ => b.is_ascii_digit(),
        });
    shape
        && format!("{} {}", &text[..10], &text[11..16])
            .parse::<totalfloat_model::DateTime>()
            .is_ok()
        && text[17..19] < *"60"
}

/// Seconds since 1970 as `YYYY-MM-DDTHH:MM:SSZ`.
fn rfc3339(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Days to a civil date, after Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

thread_local! {
    /// The agent this thread records for, while [`as_agent`] runs.
    static AGENT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    /// Whether this thread records for a person, while [`as_person`] runs.
    static PERSON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `work` recording as a person, whatever `TOTALFLOAT_AUTHOR_KIND` says:
/// `serve` runs each change from the app this way, because the app is a
/// person's.
pub fn as_person<T>(work: impl FnOnce() -> T) -> T {
    /// Puts back what was there, even if `work` panics.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            PERSON.set(self.0);
        }
    }
    let _restore = Restore(PERSON.replace(true));
    work()
}

/// Runs `work` recording as the agent `name`: changesets and reviews made in
/// it are marked as an agent's, under that name unless one is given. MCP runs
/// each tool call this way, named for its client.
pub fn as_agent<T>(name: &str, work: impl FnOnce() -> T) -> T {
    /// Puts back the agent before, even if `work` panics.
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            AGENT.with_borrow_mut(|held| *held = before);
        }
    }
    let _restore = Restore(AGENT.with_borrow_mut(|held| held.replace(name.to_owned())));
    work()
}

/// Who: `--author`, else `TOTALFLOAT_AUTHOR`, else the account's user name.
/// `TOTALFLOAT_AUTHOR_KIND=agent` marks an agent, unless inside [`as_person`],
/// and so does running inside [`as_agent`].
pub fn author(given: Option<&str>) -> (String, AuthorKind) {
    if let Some(agent) = AGENT.with_borrow(Clone::clone) {
        let name = given
            .filter(|n| !n.trim().is_empty())
            .map_or(agent, str::to_owned);
        return (name, AuthorKind::Agent);
    }
    let named = |name: String| (!name.trim().is_empty()).then_some(name);
    let name = given
        .map(str::to_owned)
        .and_then(named)
        .or_else(|| std::env::var("TOTALFLOAT_AUTHOR").ok().and_then(named))
        .or_else(|| std::env::var("USER").ok().and_then(named))
        .or_else(|| std::env::var("USERNAME").ok().and_then(named))
        .unwrap_or_else(|| "unknown".to_owned());
    let kind = match std::env::var("TOTALFLOAT_AUTHOR_KIND").as_deref() {
        Ok("agent") if !PERSON.get() => AuthorKind::Agent,
        _ => AuthorKind::Human,
    };
    (name, kind)
}

#[cfg(test)]
mod tests {
    use super::{is_utc_time, rfc3339};

    #[test]
    fn only_utc_times_are_taken() {
        assert!(is_utc_time("2026-11-02T09:00:00Z"));
        for bad in [
            "yesterday",
            "2026-11-02 09:00",
            "2026-13-02T09:00:00Z",
            "2026-11-02T09:00:61Z",
        ] {
            assert!(!is_utc_time(bad), "{bad}");
        }
    }

    #[test]
    fn seconds_read_as_civil_times() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_793_610_061), "2026-11-02T09:01:01Z");
    }
}
