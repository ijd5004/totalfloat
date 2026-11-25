//! How long a schedule takes (docs/ARCHITECTURE.md#cross-cutting-concerns).
#![forbid(unsafe_code)]
#![allow(missing_docs, reason = "criterion's macros generate public items")]

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use totalfloat_engine::{schedule, schedule_reusing};
use totalfloat_model::{ActivityId, Minutes};

#[path = "support/chains.rs"]
mod chains;
#[path = "support/network.rs"]
mod network;

fn scheduling(c: &mut Criterion) {
    let mut group = c.benchmark_group("schedule");
    group.sample_size(10);
    for activities in [1_000, 10_000, 100_000] {
        let input = network::generate(activities, 42);
        group.throughput(Throughput::Elements(activities as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(activities),
            &input,
            |b, input| {
                b.iter(|| schedule(black_box(input), &input.project.options));
            },
        );
    }
    group.finish();
}

/// One pass over the app's 50,000-activity benchmark schedule, the
/// pass every edit preview needs.
fn one_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("pass");
    group.sample_size(10);
    let input = chains::generate(50_000);
    group.throughput(Throughput::Elements(50_000));
    group.bench_function("chains-50000", |b| {
        b.iter(|| schedule(black_box(&input), &input.project.options));
    });
    group.finish();
}

/// The same schedule calculated again after one edit, taking what it can
/// from the calculation before it: a local edit, shortening an
/// activity near the end of one path by half a day, which moves the dates or
/// float of that path's thousand activities, and the app bench's edit,
/// lengthening A1, the first activity of a path, which moves the project
/// finish and so every activity's late dates and float. Every path is equally
/// long, so lengthening anything moves the finish.
fn after_an_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("reuse");
    group.sample_size(10);
    let input = chains::generate(50_000);
    let options = &input.project.options;
    // A first calculation keeps what it worked out, which a plain pass doesn't.
    group.bench_function("chains-50000-kept", |b| {
        b.iter(|| schedule_reusing(black_box(&input), options, (&[], None), None));
    });
    let Ok(before) = schedule_reusing(&input, options, (&[], None), None) else {
        return;
    };
    for (name, id, minutes) in [("local", 49_990, -240), ("a1", 1, 60 * 24)] {
        let mut edited = input.clone();
        if let Some(a) = edited.activities.get_mut(&ActivityId(id)) {
            a.remaining_duration = Minutes(a.remaining_duration.get() + minutes);
        }
        group.bench_function(format!("chains-50000-{name}"), |b| {
            b.iter(|| schedule_reusing(black_box(&edited), options, (&[], None), Some(&before)));
        });
        group.bench_function(format!("chains-50000-{name}-full"), |b| {
            b.iter(|| schedule(black_box(&edited), options));
        });
    }
    group.finish();
}

criterion_group!(benches, scheduling, one_pass, after_an_edit);
criterion_main!(benches);
