//! How long the quality checks take on the app bench's 50,000-activity
//! schedule: the "after" side of every edit preview with checks.
#![forbid(unsafe_code)]
#![allow(missing_docs, reason = "criterion's macros generate public items")]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use totalfloat_check::{Thresholds, assess, assess_calculated};
use totalfloat_engine::{schedule, schedule_with_projects_reusing};

#[path = "../../engine/benches/support/chains.rs"]
mod chains;

/// `assess` on the schedule `ui-bench` uses, with its results stored as a
/// calculated file holds them, and `assess_calculated` as an edit preview
/// calls it.
fn assessing(c: &mut Criterion) {
    let mut group = c.benchmark_group("assess");
    group.sample_size(10);
    let mut input = chains::generate(50_000);
    if let Ok(result) = schedule(&input, &input.project.options) {
        for (id, activity) in &mut input.activities {
            if let Some(found) = result.activities.get(id) {
                activity.stored = found.values.clone();
            }
        }
        input.project.stored.scheduled_finish = result.project.scheduled_finish;
    }
    let projects = std::slice::from_ref(&input);
    let thresholds = Thresholds::default();
    group.bench_function("chains-50000", |b| {
        b.iter(|| assess(black_box(&input), projects, &thresholds));
    });
    // As an edit preview assesses the edited schedule: with the calculation
    // the edit has just made.
    let options = &input.project.options;
    if let Ok(calculated) = schedule_with_projects_reusing(&input, options, projects, None) {
        group.bench_function("chains-50000-calculated", |b| {
            b.iter(|| assess_calculated(black_box(&input), projects, &thresholds, &calculated));
        });
    }
    group.finish();
}

criterion_group!(benches, assessing);
criterion_main!(benches);
