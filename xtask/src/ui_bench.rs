//! `cargo xtask ui-bench`: the web app's speed on a 50k-activity schedule
//!. It builds the app and the release binary, writes the synthetic
//! schedule `history-cost` uses, starts a history with a branch and an
//! agent's proposal, serves it, and drives a browser through
//! `ui/e2e/bench.mjs` (Node.js and playwright-cli). It prints each timing
//! against its target. First it runs the grid model's wall-clock budgets
//! (`pnpm run budget`, `ui/src/grid/model.budget.ts`), which only hold on a
//! quiet machine and so are kept out of the unit tests.

use std::error::Error;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use serde_json::Value;
use totalfloat_xer::fresh::{FreshOptions, write};

/// Stops the server when the bench ends, however it ends.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_ok(command: &mut Command, what: &str) -> Result<String, Box<dyn Error>> {
    let out = command.output()?;
    if !out.status.success() {
        return Err(format!("{what} failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A target: the timing's name in the bench's answer, its words, and the
/// most it may take.
struct Target {
    path: &'static [&'static str],
    label: &'static str,
    limit: f64,
}

/// One frame at 60 Hz. Frame times are the gaps between animation frames,
/// which a browser paces to the display's refresh, so a page that never
/// drops a frame measures this, not less: "under 16 ms" is read as "no
/// frame dropped on average", with a little room for timer jitter.
const ONE_FRAME: f64 = 17.0;

const TARGETS: [Target; 11] = [
    Target {
        path: &["firstPaintMs"],
        label: "first paint",
        limit: 3000.0,
    },
    Target {
        path: &["gridScroll", "averageMs"],
        label: "grid scroll, average frame",
        limit: ONE_FRAME,
    },
    Target {
        path: &["gridScroll", "p95Ms"],
        label: "grid scroll, 95th percentile frame",
        limit: 33.0,
    },
    Target {
        path: &["gridScrollGrouped", "averageMs"],
        label: "grouped grid scroll, average frame",
        limit: ONE_FRAME,
    },
    Target {
        path: &["gridScrollGrouped", "p95Ms"],
        label: "grouped grid scroll, 95th pct frame",
        limit: 33.0,
    },
    Target {
        path: &["ganttScroll", "averageMs"],
        label: "Gantt scroll, average frame",
        limit: ONE_FRAME,
    },
    Target {
        path: &["ganttScroll", "p95Ms"],
        label: "Gantt scroll, 95th percentile frame",
        limit: 33.0,
    },
    Target {
        path: &["editPreviewMs"],
        label: "edit preview shown",
        limit: 300.0,
    },
    Target {
        path: &["editRecordedMs"],
        label: "edit recorded, view refreshed",
        limit: 1000.0,
    },
    Target {
        path: &["branchSwitchMs"],
        label: "branch switched and shown",
        limit: f64::INFINITY,
    },
    Target {
        path: &["proposalOpenMs"],
        label: "proposal opened, comparison shown",
        limit: f64::INFINITY,
    },
];

/// Runs the bench and prints the table.
///
/// # Errors
///
/// When a build, the schedule, the server or the browser run fails.
pub(crate) fn run(activities: u64) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    crate::ui::run(crate::ui::Asked {
        check: false,
        licenses: false,
    })?;
    // Reported with the table; a missed budget, like a missed target, doesn't
    // stop the bench.
    let budgets_met = crate::ui::pnpm()
        .args(["run", "budget"])
        .current_dir(root.join("ui"))
        .status()?
        .success();
    run_ok(
        Command::new("cargo")
            .args(["build", "--release", "-q", "-p", "totalfloat-cli"])
            .current_dir(&root),
        "the release build",
    )?;
    let binary = root.join("target/release/totalfloat");
    // Under target/, so it's never committed and is replaced each run.
    let place = root.join("target/ui-bench");
    if place.exists() {
        std::fs::remove_dir_all(&place)?;
    }
    std::fs::create_dir_all(&place)?;
    let file = place.join("bench.xer");
    let xer = write(
        &crate::history_cost::model(activities)?,
        FreshOptions::new("2026-01-01".parse()?),
    )?;
    let mut bytes = Vec::new();
    xer.write(&mut bytes)?;
    std::fs::write(&file, &bytes)?;
    let store = place.join("bench.tfdb");
    let store_text = store.to_str().ok_or("path")?;
    let totalfloat = |args: &[&str], agent: bool| {
        run_ok(
            Command::new(&binary)
                .args(args)
                .current_dir(&place)
                .env(
                    "TOTALFLOAT_AUTHOR",
                    if agent { "bench-agent" } else { "bench" },
                )
                .env(
                    "TOTALFLOAT_AUTHOR_KIND",
                    if agent { "agent" } else { "human" },
                ),
            &format!("totalfloat {args:?}"),
        )
    };
    totalfloat(
        &[
            "history",
            "init",
            file.to_str().ok_or("path")?,
            "-m",
            "Bench",
        ],
        false,
    )?;
    totalfloat(&["history", "branch", store_text, "what-if"], false)?;
    // The last activity, so any count works.
    let proposed = format!("A{activities}=7d");
    totalfloat(
        &[
            "edit",
            store_text,
            "--duration",
            &proposed,
            "--propose",
            "-m",
            "Bench proposal",
        ],
        true,
    )?;

    let answer = serve_and_browse(&root, &binary, &place, store_text)?;
    let last = answer
        .lines()
        .last()
        .ok_or("the browser run printed nothing")?;
    let timings: Value = serde_json::from_str(last)?;
    print_table(activities, &timings);
    println!(
        "grid model budgets (ui/src/grid/model.budget.ts, printed above): {}",
        if budgets_met { "met" } else { "NOT met" }
    );
    match timings["error"].as_str() {
        Some(error) => Err(format!("the browser run failed: {error}").into()),
        None => Ok(()),
    }
}

/// Serves the store, runs the browser against it, and gives the browser
/// run's output. The server stops when this returns.
fn serve_and_browse(
    root: &Path,
    binary: &Path,
    place: &Path,
    store_text: &str,
) -> Result<String, Box<dyn Error>> {
    let mut child = Command::new(binary)
        .args(["serve", store_text])
        .current_dir(place)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("no output from serve")?;
    let _serving = Serving(child);
    let mut address = None;
    let mut output = BufReader::new(stdout).lines();
    for line in output.by_ref() {
        let line = line?;
        if let Some(at) = line.find("http://") {
            address = Some(line[at..].trim().to_owned());
            break;
        }
    }
    let address = address.ok_or("serve printed no address")?;
    // Read the rest of what it prints, so it never waits on a full pipe.
    std::thread::spawn(move || output.for_each(drop));
    // Not `run_ok`: a failed run still prints the timings it took.
    let out = Command::new("node")
        .arg(root.join("ui/e2e/bench.mjs"))
        .arg(&address)
        .current_dir(root)
        .stderr(Stdio::inherit())
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn print_table(activities: u64, timings: &Value) {
    println!(
        "{activities} activities, {} rows with the WBS bands; {}",
        timings["rows"],
        timings["browser"].as_str().unwrap_or("browser unknown")
    );
    println!("{:<40} {:>10} {:>10}  met", "measure", "ms", "target");
    let get = |path: &[&str]| path.iter().fold(timings, |v, key| &v[*key]).as_f64();
    for target in &TARGETS {
        let value = get(target.path);
        let (limit, met) = if target.limit.is_finite() {
            let met = match value {
                Some(v) if v <= target.limit => "yes",
                Some(_) => "NO",
                None => "?",
            };
            (format!("{:.0}", target.limit), met)
        } else {
            ("none".to_owned(), "-")
        };
        let shown = value.map_or_else(|| "?".to_owned(), |v| format!("{v:.1}"));
        println!("{:<40} {shown:>10} {limit:>10}  {met}", target.label);
    }
    for (name, key) in [("grid", "gridScroll"), ("Gantt", "ganttScroll")] {
        println!(
            "{name} scroll: {} frames, worst {} ms",
            timings[key]["frames"], timings[key]["worstMs"]
        );
    }
}
