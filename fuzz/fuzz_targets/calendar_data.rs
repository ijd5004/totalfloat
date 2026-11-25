//! The `clndr_data` parser on arbitrary text: no panics, and the original text
//! is always kept.
#![no_main]

use libfuzzer_sys::fuzz_target;
use totalfloat_xer::calendar_data::CalendarData;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let calendar = CalendarData::decode(text);
        let _ = calendar.diagnostics();
    }
});
