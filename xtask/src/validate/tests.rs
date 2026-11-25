use super::*;

// ---- XER Tools' summary line ----

#[test]
fn a_clean_run_has_no_warnings_or_errors() {
    let log = "x\u{1}  Checking table \"TASK\" integrity: \u{1}2\n\
               46341.04\u{1}107 of 107 Checks completed without warnings or errors!\u{1}7\n\
               46341.05\u{1}XER Tools 2.2b session finished!\u{1}2\n";
    assert_eq!(
        integrity(log),
        Some(Integrity {
            completed: 107,
            of: 107,
            warnings: 0,
            errors: 0
        })
    );
}

#[test]
fn warnings_and_errors_are_counted_with_thousands_separators() {
    let log = "46341.04\u{fffd}3,553,729 of 3,553,449  checks completed with 4,508 warnings and 5 errors!\u{fffd}7\n";
    assert_eq!(
        integrity(log),
        Some(Integrity {
            completed: 3_553_729,
            of: 3_553_449,
            warnings: 4_508,
            errors: 5
        })
    );
}

#[test]
fn one_warning_and_no_errors() {
    let log = "115 of 115 checks completed with 1 warning and no errors!\n";
    let summary = integrity(log).unwrap();
    assert_eq!((summary.warnings, summary.errors), (1, 0));
}

#[test]
fn a_log_without_a_summary_has_none() {
    assert_eq!(integrity("Loading table data for \"TASK\"\n"), None);
}

#[test]
fn a_count_that_cannot_be_read_is_no_summary() {
    assert_eq!(integrity("14 of 14 checks completed with errors!\n"), None);
}

#[test]
fn checks_cut_short_do_not_pass() {
    let summary = |completed, errors| Integrity {
        completed,
        of: 3_553_449,
        warnings: 0,
        errors,
    };
    assert!(matches!(
        verdict(summary(40_000, 0), String::new()),
        Outcome::Failed(why) if why.contains("didn't finish")
    ));
    assert!(matches!(
        verdict(summary(3_553_729, 0), String::new()),
        Outcome::Passed(_)
    ));
    assert!(matches!(
        verdict(summary(3_553_729, 5), String::new()),
        Outcome::Failed(_)
    ));
}

#[test]
fn only_the_last_session_counts() {
    let clean = "46343.53\u{1}XER Tools 2.2b session started...\u{1}0\n\
                 46343.53\u{1}182 of 172 Checks completed without warnings or errors!\u{1}3\n\
                 46343.54\u{1}XER Tools 2.2b session finished!\u{1}2\n";
    let failed_load = "46343.55\u{1}XER Tools 2.2b session started...\u{1}0\n\
                       46343.55\u{1}Error loading file\u{1}7\n\
                       46343.56\u{1}XER Tools 2.2b session finished!\u{1}2\n";
    assert_eq!(last_session(clean).map(|s| s.errors), Some(0));
    assert_eq!(last_session(&format!("{clean}{failed_load}")), None);
}

#[test]
fn an_unfinished_session_has_no_summary() {
    let log = "XER Tools 2.2b session started...\n\
               14 of 14  checks completed with 2 errors!\n";
    assert_eq!(last_session(log), None);
}

// ---- Counts ----

#[test]
fn counts_read_key_value_lines() {
    let counts = counts("activities 2\nlinks 1\nnoise\nresources x\n");
    assert_eq!(counts.get("activities"), Some(&2));
    assert_eq!(counts.get("links"), Some(&1));
    assert_eq!(counts.get("resources"), None);
}

#[test]
fn differences_name_each_count_that_differs() {
    let ours = Counts::from([("links".into(), 14), ("activities".into(), 16)]);
    let theirs = Counts::from([
        ("links".into(), 17),
        ("activities".into(), 16),
        ("summaries".into(), 4),
    ]);
    assert_eq!(
        differences(&ours, &theirs),
        vec!["links: TotalFloat 14, MPXJ 17".to_owned()]
    );
    assert_eq!(differences(&ours, &ours), Vec::<String>::new());
}

#[test]
fn a_count_mpxj_did_not_print_differs() {
    let ours = Counts::from([("calendars".into(), 0)]);
    assert_eq!(
        differences(&ours, &Counts::new()),
        vec!["calendars: TotalFloat 0, MPXJ none".to_owned()]
    );
}

#[test]
fn what_the_reader_left_out_counts_toward_the_comparison() {
    let kept = Counts::from([("links".into(), 14), ("activities".into(), 16)]);
    let notes = [("summary-logic", 3), ("name-shortened", 2)];
    let (counts, explained) = accounted(kept, &notes);
    assert_eq!(counts.get("links"), Some(&17));
    assert_eq!(counts.get("activities"), Some(&16));
    assert_eq!(explained, vec!["3 links (summary-logic)".to_owned()]);
}

// ---- Schema assembly ----

fn page(xml: &str) -> String {
    format!("---\ntitle: x\n---\n\n# Section\n\n``` xml\n{xml}\n```\n\nMore text.\n")
}

fn pages() -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (
            "Project",
            page(
                "<?xml version=\"1.0\"?>\n\
                 <xsd:schema xmlns=\"http://schemas.microsoft.com/project/2007\" targetNamespace=\"http://schemas.microsoft.com/project/2007\">\n\
                 <xsd:complexType name=\"TimephasedDataType\"> . . . </xsd:complexType>\n\
                 <xsd:element name=\"Project\"><xsd:complexType><xsd:sequence>\n\
                 <xsd:element name=\"Tasks\" minOccurs=\"0\"> . . . </xsd:element>\n\
                 </xsd:sequence></xsd:complexType></xsd:element>",
            ),
        ),
        (
            "Tasks",
            page(
                "<?xml version=\"1.0\"?>\n. . .\n<!-- @Task schema -->\n\
                 <xsd:element name=\"Tasks\" minOccurs=\"0\"><xsd:complexType/></xsd:element>",
            ),
        ),
        (
            "TimephasedDataType",
            page("<xsd:complexType name=\"TimephasedDataType\"><xsd:sequence/></xsd:complexType>"),
        ),
    ])
}

#[test]
fn sections_fill_the_elided_definitions() {
    let schema = assemble(&pages()).unwrap();
    assert!(schema.contains("<xsd:element name=\"Tasks\" minOccurs=\"0\"><xsd:complexType/>"));
    assert!(schema.contains("<xsd:complexType name=\"TimephasedDataType\"><xsd:sequence/>"));
    assert!(!schema.contains(". . ."), "{schema}");
    assert!(schema.trim_end().ends_with("</xsd:schema>"), "{schema}");
}

#[test]
fn the_schema_takes_the_namespace_files_use() {
    let schema = assemble(&pages()).unwrap();
    assert!(schema.contains("targetNamespace=\"http://schemas.microsoft.com/project\""));
    assert!(!schema.contains("project/2007"), "{schema}");
}

#[test]
fn a_missing_or_misplaced_section_is_refused() {
    let mut missing = pages();
    missing.remove("Tasks");
    assert!(assemble(&missing).unwrap_err().contains("still elided"));

    let mut extra = pages();
    extra.insert(
        "Resources",
        page("<xsd:element name=\"Resources\" minOccurs=\"0\"/>"),
    );
    assert!(
        assemble(&extra)
            .unwrap_err()
            .contains("Resources: the Project section has no place for it")
    );
}

// ---- Missing tools ----

#[test]
fn every_check_is_skipped_without_its_tools() {
    let empty = tempfile::tempdir().unwrap();
    let tools = Tools::at(empty.path());
    let file = empty.path().join("plan.xer");
    std::fs::write(&file, "ERMHDR\t23.12\n").unwrap();
    for outcome in [
        xertools(&tools, &file).unwrap(),
        mpxj_parses(&tools, &file),
        schema(&tools, &file),
        mpxj_read_back(&tools, &file),
    ] {
        assert!(matches!(outcome, Outcome::Skipped(_)), "{outcome:?}");
    }
}
