//! Raw reader and writer: any bytes either fail cleanly or round-trip exactly.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(raw) = totalfloat_xer::read(data) {
        let mut out = Vec::new();
        raw.write(&mut out).unwrap();
        assert_eq!(out, data, "unedited files must write back byte-identical");
    }
});
