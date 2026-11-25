//! Typed decoding, calendar resolution and model mapping on any readable XER:
//! no panics, and every mapped schedule passes the patch writer's no-op check.
#![no_main]

use libfuzzer_sys::fuzz_target;
use totalfloat_xer::typed::TypedXer;

fuzz_target!(|data: &[u8]| {
    let Ok(raw) = totalfloat_xer::read(data) else {
        return;
    };
    let typed = TypedXer::decode(&raw);
    drop(typed);
    let (schedules, _report) = totalfloat_xer::to_model(&raw);
    for schedule in &schedules {
        let _ = schedule.validate();
        if let Ok(patched) = totalfloat_xer::patch(&raw, schedule) {
            let mut out = Vec::new();
            patched.write(&mut out).unwrap();
            assert_eq!(out, data, "patching an unmodified schedule must change nothing");
        }
    }
});
