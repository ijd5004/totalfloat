//! The benchmark network has to be one the engine actually finishes.
#![forbid(unsafe_code)]

#[path = "../benches/support/network.rs"]
mod network;

use totalfloat_engine::schedule;

#[test]
fn generated_network_is_seeded_realistic_and_schedulable() {
    let input = network::generate(10_000, 42);
    assert_eq!(input, network::generate(10_000, 42));
    assert_ne!(input, network::generate(10_000, 43));
    assert_eq!(input.activities.len(), 10_000);
    assert!((14_000..16_000).contains(&input.relationships.len()));
    assert_eq!(input.calendars.len(), 3);
    assert!(input.wbs.len() > 100);
    assert_eq!(input.validate(), []);

    let result = schedule(&input, &input.project.options).unwrap();
    assert_eq!(result.activities.len(), 10_000);
    assert!(result.project.scheduled_finish.is_some());
}
