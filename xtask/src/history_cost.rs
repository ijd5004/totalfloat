//! `cargo xtask history-cost`: what recording edits in a history store costs
//! on a 50,000-activity schedule, against the same edits file to file
//! (ARCHITECTURE.md, History and changesets). Builds a synthetic schedule, then times the release
//! binary.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};

/// Recording may add at most this much to an edit's time.
const GUARD: f64 = 1.25;

fn calendar(id: m::CalendarId) -> Result<m::Calendar, Box<dyn Error>> {
    let time = |hour| m::TimeOfDay::from_hm(hour, 0).ok_or("bad hour");
    let morning = m::WorkInterval::new(time(8)?, time(12)?).ok_or("bad interval")?;
    let afternoon = m::WorkInterval::new(time(13)?, time(17)?).ok_or("bad interval")?;
    Ok(m::Calendar {
        id,
        name: "Five-day".to_owned(),
        kind: m::CalendarType::Global,
        project: None,
        base: None,
        is_default: true,
        hours_per_period: m::HoursPerPeriod {
            day: Some(m::Minutes(480)),
            week: Some(m::Minutes(2400)),
            month: Some(m::Minutes(9600)),
            year: Some(m::Minutes(115_200)),
        },
        work_week: m::WorkWeek(std::array::from_fn(|day| {
            if (1..=5).contains(&day) {
                vec![morning, afternoon]
            } else {
                vec![]
            }
        })),
        exceptions: BTreeMap::new(),
        source: None,
    })
}

fn wbs(id: u64, project: m::ProjectId, parent: Option<m::WbsId>, code: String) -> m::Wbs {
    m::Wbs {
        id: m::WbsId(id),
        project,
        parent,
        code: code.clone(),
        name: code,
        sequence: 0,
        is_project_node: parent.is_none(),
        earned_value: m::EarnedValue::default(),
        obs: None,
        notes: std::collections::BTreeMap::new(),
        udfs: BTreeMap::new(),
        source: None,
    }
}

/// `activities` activities in chains of 100 finish-to-start links, each
/// chain under its own WBS node, and each chain linked to the one before
/// except every tenth, so the paths are ten chains long.
pub(crate) fn model(activities: u64) -> Result<m::Schedule, Box<dyn Error>> {
    let project_id = m::ProjectId(1);
    let calendar_id = m::CalendarId(1);
    let mut project = m::Project::new(project_id, "COST");
    project.name = Some("History cost benchmark".to_owned());
    project.data_date = Some("2026-01-05 08:00".parse()?);
    project.planned_start = project.data_date;
    project.default_calendar = Some(calendar_id);
    let mut schedule = m::Schedule::new(project);
    schedule
        .calendars
        .insert(calendar_id, calendar(calendar_id)?);
    // The fresh writer needs the root node named as the project is.
    let mut root = wbs(1, project_id, None, "COST".to_owned());
    "History cost benchmark".clone_into(&mut root.name);
    schedule.wbs.insert(root.id, root);
    let mut links = 0;
    let mut link = |schedule: &mut m::Schedule, from: u64, to: u64| {
        links += 1;
        let id = m::RelationshipId(links);
        schedule.relationships.insert(
            id,
            m::Relationship {
                id,
                predecessor: m::ActivityId(from),
                successor: m::ActivityId(to),
                predecessor_project: project_id,
                successor_project: project_id,
                kind: m::RelationshipType::FinishToStart,
                lag: m::Minutes::ZERO,
                source: None,
            },
        );
    };
    for n in 1..=activities {
        let chain = (n - 1) / 100;
        let node = 10 + chain;
        if (n - 1) % 100 == 0 {
            let child = wbs(node, project_id, Some(m::WbsId(1)), format!("C{chain}"));
            schedule.wbs.insert(child.id, child);
        }
        let code = format!("A{n}");
        let mut activity = m::Activity::new(
            m::ActivityId(n),
            project_id,
            m::WbsId(node),
            calendar_id,
            code,
            "Work",
        );
        let hours = 8 * (1 + i64::try_from(n % 5)?);
        activity.original_duration = m::Minutes(hours * 60);
        activity.remaining_duration = activity.original_duration;
        schedule.activities.insert(activity.id, activity);
        if (n - 1) % 100 != 0 {
            link(&mut schedule, n - 1, n);
        }
        if (n - 1) % 100 == 0 && chain % 10 != 0 {
            link(&mut schedule, n - 1, n);
        }
    }
    Ok(schedule)
}

fn timed(binary: &Path, args: &[&str]) -> Result<Duration, Box<dyn Error>> {
    let started = Instant::now();
    let out = Command::new(binary)
        .args(args)
        .env("TOTALFLOAT_AUTHOR", "bench")
        .output()?;
    let elapsed = started.elapsed();
    if !out.status.success() {
        return Err(format!(
            "{} {args:?} failed: {}",
            binary.display(),
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(elapsed)
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// Runs the measurement and prints it.
#[allow(
    clippy::cast_precision_loss,
    reason = "sizes and counts shown as approximate ratios and megabytes"
)]
///
/// # Errors
///
/// When building, generating or a timed run fails, or when recording adds
/// more than the guard allows.
pub(crate) fn run(activities: u64, edits: usize) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let built = Command::new("cargo")
        .args(["build", "--release", "-q", "-p", "totalfloat-cli"])
        .current_dir(&root)
        .status()?;
    if !built.success() {
        return Err("the release build failed".into());
    }
    let binary = root.join("target/release/totalfloat");
    // Under target/, so it's never committed and is replaced each run.
    let place = root.join("target/history-cost");
    if place.exists() {
        std::fs::remove_dir_all(&place)?;
    }
    std::fs::create_dir_all(&place)?;
    let file = place.join("cost.xer");
    let xer = write(
        &model(activities)?,
        FreshOptions::new("2026-01-01".parse()?),
    )?;
    let mut bytes = Vec::new();
    xer.write(&mut bytes)?;
    std::fs::write(&file, &bytes)?;
    let file_text = file.to_str().ok_or("path")?;
    println!(
        "{activities} activities, {} MB as XER",
        bytes.len() / 1_000_000
    );
    let store = place.join("cost.tfdb");
    let store_text = store.to_str().ok_or("path")?;
    let init = timed(&binary, &["history", "init", file_text])?;
    // File and store edits take turns, so a busy moment hits both alike.
    let (mut plain, mut recorded) = (Vec::new(), Vec::new());
    let mut previous = file.clone();
    for n in 0..edits {
        let edit = format!("A{}=3d", 1 + n * 97);
        let out = place.join(format!("plain-{n}.xer"));
        let input = previous.to_str().ok_or("path")?.to_owned();
        let args = [
            "edit",
            &input,
            "--duration",
            &edit,
            "-o",
            out.to_str().ok_or("path")?,
        ];
        plain.push(timed(&binary, &args)?);
        previous = out;
        recorded.push(timed(
            &binary,
            &["edit", store_text, "--duration", &edit, "-m", "cost"],
        )?);
    }
    let size = std::fs::metadata(&store)?.len();
    let (plain, recorded) = (median(plain), median(recorded));
    let ratio = recorded.as_secs_f64() / plain.as_secs_f64();
    let raw = bytes.len() as f64 * (edits + 1) as f64;
    println!("history init: {:.2} s", init.as_secs_f64());
    println!(
        "edit file to file, median of {edits}: {:.2} s",
        plain.as_secs_f64()
    );
    println!(
        "edit recorded in the store, median of {edits}: {:.2} s",
        recorded.as_secs_f64()
    );
    println!(
        "recording adds {:.0}% (guard {:.0}%)",
        (ratio - 1.0) * 100.0,
        (GUARD - 1.0) * 100.0
    );
    println!(
        "store: {:.1} MB for {} versions, {:.1}x smaller than the files",
        size as f64 / 1e6,
        edits + 1,
        raw / size as f64
    );
    if ratio > GUARD {
        return Err(format!(
            "recording adds {:.0}%, more than the guard",
            (ratio - 1.0) * 100.0
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_benchmark_schedule_writes() {
        let options = FreshOptions::new("2026-01-01".parse().unwrap());
        assert!(write(&model(250).unwrap(), options).is_ok());
    }
}
