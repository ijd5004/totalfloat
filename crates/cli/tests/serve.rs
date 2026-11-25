//! `totalfloat serve`: the local app's read API, over a real port, signed in as
//! a browser would be.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

/// Runs the binary to completion with a fixed author and time.
fn run(args: &[&str]) -> (Option<i32>, String, String) {
    run_with(args, &[])
}

/// [`run`], with more of the environment set.
fn run_with(args: &[&str], env: &[(&str, &str)]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(args)
        .env("TOTALFLOAT_NOW", "2026-11-02T09:00:00Z")
        .env("TOTALFLOAT_AUTHOR", "Pat")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// A running `serve`, signed in. Stopped when dropped.
struct Served {
    child: Child,
    port: u16,
    cookie: String,
    token: String,
    printed: String,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One HTTP exchange; the answer's status, headers and body.
fn exchange(port: u16, request: &str) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).unwrap();
    let split = answer.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(answer[..split].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head, answer[split + 4..].to_vec())
}

/// Starts `serve` on `location` and signs in with the link it prints.
fn serve(location: &str) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(["serve", location])
        // The app is a person's, whatever the environment says.
        .env("TOTALFLOAT_AUTHOR", "Pat")
        .env("TOTALFLOAT_AUTHOR_KIND", "agent")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut printed = String::new();
    let url = loop {
        let line = lines.next().unwrap().unwrap();
        printed.push_str(&line);
        printed.push('\n');
        if let Some(url) = line.trim().strip_prefix("http://") {
            break url.to_owned();
        }
    };
    let (authority, query) = url.split_once("/?token=").unwrap();
    let port = authority.rsplit_once(':').unwrap().1.parse().unwrap();
    let token = query.to_owned();
    let request =
        format!("GET /?token={token} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    let (status, head, _) = exchange(port, &request);
    assert_eq!(status, 303, "{head}");
    let cookie = head
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    Served {
        child,
        port,
        cookie,
        token,
        printed,
    }
}

/// An open event stream, as a page holds one.
struct Feed {
    reader: BufReader<TcpStream>,
}

impl Feed {
    /// The next event named `kind`, skipping others, before `deadline`.
    fn next(&mut self, kind: &str, deadline: Instant) -> Value {
        let mut name = String::new();
        let mut data = String::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "no {kind} event in time");
            self.reader.get_ref().set_read_timeout(Some(left)).unwrap();
            let mut line = String::new();
            assert!(
                self.reader.read_line(&mut line).unwrap() > 0,
                "the stream ended"
            );
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(value) = line.strip_prefix("event:") {
                value.trim().clone_into(&mut name);
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.trim_start());
            } else if line.is_empty() {
                if name == kind {
                    return serde_json::from_str(&data).unwrap();
                }
                name.clear();
                data.clear();
            }
        }
    }
}

impl Served {
    /// Opens the event stream, as a page's `EventSource` does, and waits until
    /// it's listening.
    fn feed(&self) -> Feed {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        // HTTP/1.0, so the stream comes unchunked, line by line.
        let request = format!(
            "GET /api/1/events?session={} HTTP/1.0\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\n\r\n",
            self.token, self.port, self.cookie
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut reader = BufReader::new(stream);
        let mut status = String::new();
        reader.read_line(&mut status).unwrap();
        assert!(status.contains(" 200 "), "{status}");
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
        }
        Feed { reader }
    }

    /// `GET path`, signed in; the status and the JSON answer.
    fn get(&self, path: &str) -> (u16, Value) {
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\nx-totalfloat-session: {}\r\nConnection: close\r\n\r\n",
            self.port, self.cookie, self.token
        );
        let (status, head, body) = exchange(self.port, &request);
        assert!(head.contains("content-type: application/json"), "{head}");
        (status, serde_json::from_slice(&body).unwrap())
    }

    /// `GET path`, signed in; the status, headers and raw body.
    fn download(&self, path: &str) -> (u16, String, Vec<u8>) {
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\nx-totalfloat-session: {}\r\nConnection: close\r\n\r\n",
            self.port, self.cookie, self.token
        );
        exchange(self.port, &request)
    }

    /// `POST path` with a JSON body from this page's origin, signed in; the
    /// status, the head it reports, and the JSON answer.
    fn post(&self, path: &str, body: &Value) -> (u16, Option<String>, Value) {
        let body = body.to_string();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nCookie: {}\r\nx-totalfloat-session: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.cookie,
            self.token,
            body.len(),
            port = self.port,
        );
        let (status, head, answer) = exchange(self.port, &request);
        let moved = head
            .lines()
            .find_map(|line| line.strip_prefix("x-totalfloat-head: "))
            .map(str::to_owned);
        (status, moved, serde_json::from_slice(&answer).unwrap())
    }

    /// The head the page sees now.
    fn head(&self) -> String {
        let (_, summary) = self.get("/api/1/summary");
        summary["head"].as_str().unwrap().to_owned()
    }
}

fn lengthen(activity: &str, duration: &str) -> Value {
    serde_json::json!([{"op": "set_duration", "activity": activity, "original": duration}])
}

/// A copy of the fixture with its history started, and A1 lengthened since.
fn store(directory: &Path) -> String {
    let file = directory.join("plant.xer");
    std::fs::copy(FIXTURE, &file).unwrap();
    let (code, _, stderr) = run(&[
        "history",
        "init",
        file.to_str().unwrap(),
        "-m",
        "As submitted",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let store = directory.join("plant.tfdb").to_str().unwrap().to_owned();
    let (code, _, stderr) = run(&["edit", &store, "--duration", "A1=3d", "-m", "Steel late"]);
    assert_eq!(code, Some(0), "{stderr}");
    store
}

/// Column `name` of a columnar answer, keyed by Activity ID.
fn by_code(rows: &Value, name: &str) -> std::collections::BTreeMap<String, Value> {
    let codes = rows["code"].as_array().unwrap();
    let values = rows[name].as_array().unwrap();
    assert_eq!(codes.len(), values.len(), "{name}");
    codes
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .zip(values.iter().cloned())
        .collect()
}

#[test]
fn rows_list_every_activity_with_the_dates_schedule_gives() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let (status, rows) = served.get("/api/1/rows");
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows["schema"], "totalfloat.rows/1");

    // The same calculation, through `schedule` and `query`.
    let head = directory.path().join("head.xer");
    let calculated = directory.path().join("calculated.xer");
    run(&["history", "export", &store, "-o", head.to_str().unwrap()]);
    let (code, _, stderr) = run(&[
        "schedule",
        head.to_str().unwrap(),
        "-o",
        calculated.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let (code, stdout, stderr) = run(&[
        "query",
        calculated.to_str().unwrap(),
        "--columns",
        "code,name,status,type,calendar,constraint,constraint_date,early_start,early_finish,late_start,late_finish,actual_start,actual_finish,original_duration_hours,remaining_duration_hours,total_float_hours,free_float_hours,critical",
        "--limit",
        "200",
        "--json",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let query: Value = serde_json::from_str(&stdout).unwrap();
    let listed = query["rows"].as_array().unwrap();
    assert_eq!(rows["count"], listed.len());
    assert_eq!(rows["code"].as_array().unwrap().len(), listed.len());
    for column in [
        "name",
        "status",
        "type",
        "calendar",
        "constraint",
        "constraint_date",
        "early_start",
        "early_finish",
        "late_start",
        "late_finish",
        "actual_start",
        "actual_finish",
        "original_duration_hours",
        "remaining_duration_hours",
        "total_float_hours",
        "free_float_hours",
        "critical",
    ] {
        let served = by_code(&rows, column);
        for row in listed {
            let code = row["code"].as_str().unwrap();
            assert_eq!(served[code], row[column], "{code} {column}");
        }
    }
    assert_eq!(by_code(&rows, "original_duration_hours")["A1"], 24.0);

    let (status, summary) = served.get("/api/1/summary");
    assert_eq!(status, 200);
    assert_eq!(summary["branch"], "main");
    assert_eq!(summary["writable"], true);
    assert_eq!(summary["calculated"], true);
    assert_eq!(summary["inspect"]["schema"], "totalfloat.inspect/1");
}

#[test]
fn at_reads_the_original_import_without_moving_the_view() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let (_, stdout, _) = run(&["history", "log", &store, "--json"]);
    let log: Value = serde_json::from_str(&stdout).unwrap();
    let changesets = log["changesets"].as_array().unwrap();
    let import = changesets.last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let original = {
        let (code, stdout, _) = run(&[
            "query",
            FIXTURE,
            "--columns",
            "code,original_duration_hours",
            "--json",
        ]);
        assert_eq!(code, Some(0));
        let doc: Value = serde_json::from_str(&stdout).unwrap();
        doc["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["code"] == "A1")
            .unwrap()["original_duration_hours"]
            .clone()
    };
    assert_ne!(original, 24.0);

    let served = serve(&store);
    let (status, rows) = served.get(&format!("/api/1/rows?at={import}"));
    assert_eq!(status, 200, "{rows}");
    assert_eq!(by_code(&rows, "original_duration_hours")["A1"], original);
    let (_, summary) = served.get(&format!("/api/1/summary?at={import}"));
    assert_eq!(summary["head"], import);
    assert_eq!(summary["branch"], Value::Null);
    assert_eq!(summary["writable"], false);
    // The view itself stays on the branch.
    let (_, rows) = served.get("/api/1/rows");
    assert_eq!(by_code(&rows, "original_duration_hours")["A1"], 24.0);
    let (_, summary) = served.get("/api/1/summary?at=main");
    assert_eq!(summary["branch"], "main");
    assert_eq!(
        summary["writable"], false,
        "a point read with at is never written"
    );
    let (status, refused) = served.get("/api/1/rows?at=nowhere");
    assert_eq!(status, 404);
    assert_eq!(refused["error"]["code"], "no-such-ref");
}

#[test]
fn a_plain_xer_serves_read_only() {
    let served = serve(FIXTURE);
    assert!(served.printed.contains("read-only"), "{}", served.printed);
    let (status, summary) = served.get("/api/1/summary");
    assert_eq!(status, 200);
    assert_eq!(summary["writable"], false);
    assert_eq!(summary["branch"], Value::Null);
    assert_eq!(summary["head"], Value::Null);
    let (status, refused) = served.get("/api/1/rows?at=main");
    assert_eq!(status, 400);
    assert_eq!(refused["schema"], "totalfloat.error/1");
    assert_eq!(refused["error"]["code"], "no-history");
}

#[test]
fn links_wbs_explain_and_check_answer() {
    let served = serve(FIXTURE);
    let (status, links) = served.get("/api/1/links");
    assert_eq!(status, 200, "{links}");
    let count = links["count"].as_u64().unwrap();
    assert!(count > 0);
    for column in [
        "id",
        "predecessor",
        "successor",
        "type",
        "lag_hours",
        "driving",
    ] {
        assert_eq!(
            links[column].as_array().unwrap().len() as u64,
            count,
            "{column}"
        );
    }
    assert!(
        links["driving"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d == true)
    );

    let (status, wbs) = served.get("/api/1/wbs");
    assert_eq!(status, 200);
    assert_eq!(wbs["schema"], "totalfloat.wbs/1");
    assert!(
        !wbs["projects"][0]["nodes"].as_array().unwrap().is_empty(),
        "{wbs}"
    );

    let (status, explained) = served.get("/api/1/explain?activity=A2&field=early_finish");
    assert_eq!(status, 200, "{explained}");
    assert_eq!(explained["schema"], "totalfloat.explain/1");
    assert_eq!(explained["field"], "early_finish");
    // What the page's driving path reads: each step's activity, and the
    // relationship that drove it, which is one of the links served.
    let steps = explained["chains"][0]["steps"].as_array().unwrap();
    assert!(
        steps.iter().all(|step| step["activity"].is_u64()),
        "{explained}"
    );
    // And what set each date: its kind, and for a relationship its type,
    // lag and the activity at the other end.
    assert!(
        steps.iter().all(|step| step["field"].is_string()
            && step["cause"]["kind"].is_string()
            && step["hours_per_day"].is_number()),
        "{explained}"
    );
    let link = steps
        .iter()
        .find(|step| step["cause"]["kind"] == "relationship")
        .unwrap();
    for key in ["link", "from_field", "from_role"] {
        assert!(link["cause"][key].is_string(), "{key}: {link}");
    }
    assert!(
        link["cause"]["from"].is_u64() && link["cause"]["lag_hours"].is_number(),
        "{link}"
    );
    let (_, links) = served.get("/api/1/links");
    let ids = links["id"].as_array().unwrap();
    let driven: Vec<&Value> = steps
        .iter()
        .filter_map(|step| step.get("relationship"))
        .collect();
    assert!(!driven.is_empty(), "{explained}");
    assert!(
        driven.iter().all(|id| ids.contains(id)),
        "{explained} {links}"
    );
    let (status, refused) = served.get("/api/1/explain?activity=NOPE");
    assert_eq!(status, 404, "{refused}");
    let (status, refused) = served.get("/api/1/explain");
    assert_eq!(status, 400);
    assert_eq!(refused["error"]["code"], "missing-argument");

    let (status, checked) = served.get("/api/1/check");
    assert_eq!(status, 200);
    assert_eq!(checked["schema"], "totalfloat.check/1");

    let (status, refused) = served.get("/api/1/rows?colour=blue");
    assert_eq!(status, 400);
    assert_eq!(refused["error"]["code"], "unknown-argument");
}

#[test]
fn the_api_needs_the_session() {
    let served = serve(FIXTURE);
    let request = format!(
        "GET /api/1/rows HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\nConnection: close\r\n\r\n",
        served.port, served.cookie
    );
    let (status, _, _) = exchange(served.port, &request);
    assert_eq!(status, 401);
}

#[test]
fn rows_match_the_published_schema() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string("../../docs/schemas/rows-1.schema.json").unwrap(),
    )
    .unwrap();
    let served = serve(FIXTURE);
    let (_, rows) = served.get("/api/1/rows");
    let mut fields: Vec<&String> = rows.as_object().unwrap().keys().collect();
    let mut published: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
    fields.sort();
    published.sort();
    assert_eq!(fields, published);
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    required.sort_unstable();
    assert_eq!(
        required,
        fields.iter().map(|f| f.as_str()).collect::<Vec<_>>()
    );
}

#[cfg(unix)]
#[test]
fn ctrl_c_stops_it_cleanly() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(["serve", FIXTURE, "--port", "0"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let port: u16 = loop {
        let line = lines.next().unwrap().unwrap();
        if let Some(url) = line.trim().strip_prefix("http://127.0.0.1:") {
            break url.split('/').next().unwrap().parse().unwrap();
        }
    };
    let request = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    assert_eq!(exchange(port, &request).0, 200);
    let interrupted = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(interrupted.success());
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
    let rest: Vec<String> = lines.map(Result::unwrap).collect();
    assert_eq!(
        rest.last().map(String::as_str),
        Some("Stopped."),
        "{rest:?}"
    );
}

#[test]
fn an_edit_records_a_changeset_authored_by_the_person() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let before = served.head();
    let body = serde_json::json!({"edits": lengthen("A2", "32h"), "head": before, "message": "Crane late"});
    let (status, moved, report) = served.post("/api/1/edit", &body);
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["schema"], "totalfloat.edit/1");
    let moved = moved.unwrap();
    assert_ne!(moved, before);
    assert_eq!(served.head(), moved);
    let (_, rows) = served.get("/api/1/rows");
    assert_eq!(by_code(&rows, "original_duration_hours")["A2"], 32.0);
    let (status, log) = served.get("/api/1/history/log?limit=1");
    assert_eq!(status, 200, "{log}");
    let latest = &log["changesets"][0];
    assert_eq!(latest["id"], moved.as_str());
    assert_eq!(latest["author"], "Pat");
    assert_eq!(latest["author_kind"], "human");
    assert_eq!(latest["message"], "Crane late");
}

/// The fields ui/src/edit/editing.ts reads from an edit's report, with their
/// JSON types, on a dry run and when recorded. Change both together.
#[test]
fn the_edit_report_has_what_the_page_reads() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let head = served.head();
    let preview =
        serde_json::json!({"edits": lengthen("A2", "32h"), "head": head, "dry_run": true});
    let (status, _, report) = served.post("/api/1/edit", &preview);
    assert_eq!(status, 200, "{report}");
    assert!(report.get("recorded").is_none(), "{report}");
    let check = |report: &Value| {
        assert!(
            report["applied"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
            "{report}"
        );
        assert!(
            report["warnings"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
            "{report}"
        );
        let project = &report["projects"][0];
        assert!(
            project["id"].is_u64() && project["code"].is_string(),
            "{project}"
        );
        for key in ["scheduled_finish", "previous_finish"] {
            assert!(
                project[key].is_string() || project[key].is_null(),
                "{key}: {project}"
            );
        }
        for key in [
            "changed_dates_or_float",
            "became_critical_total",
            "left_critical_total",
        ] {
            assert!(project[key].is_u64(), "{key}: {project}");
        }
    };
    check(&report);

    let record = serde_json::json!({"edits": lengthen("A2", "32h"), "head": head});
    let (status, _, report) = served.post("/api/1/edit", &record);
    assert_eq!(status, 200, "{report}");
    check(&report);
    let recorded = &report["recorded"];
    for key in ["id", "short_id", "branch"] {
        assert!(recorded[key].is_string(), "{key}: {recorded}");
    }
}

/// A preview is answered from the schedule `serve` already holds, and
/// says exactly what `edit --dry-run --json` says for the same edit: on the
/// branch's only project, on a head with no results stored, with a data date
/// and checks, with an edit skipped, and on a file of two projects.
#[test]
fn a_preview_is_the_command_line_s_dry_run() {
    let directory = tempfile::tempdir().unwrap();
    let one = store(directory.path());
    let two = {
        let file = directory.path().join("pair.xer");
        std::fs::copy("../../fixtures/xer/two-projects.xer", &file).unwrap();
        let (code, _, stderr) = run(&["history", "init", file.to_str().unwrap()]);
        assert_eq!(code, Some(0), "{stderr}");
        directory
            .path()
            .join("pair.tfdb")
            .to_str()
            .unwrap()
            .to_owned()
    };
    // Batch 001 holds no results, so here what was read and what's
    // calculated differ, and a preview must measure against the calculation.
    let unscheduled = {
        let place = directory.path().join("unscheduled");
        std::fs::create_dir(&place).unwrap();
        let file = place.join("plain.xer");
        std::fs::copy(FIXTURE, &file).unwrap();
        let (code, _, stderr) = run(&["history", "init", file.to_str().unwrap()]);
        assert_eq!(code, Some(0), "{stderr}");
        place.join("plain.tfdb").to_str().unwrap().to_owned()
    };
    let two_edits = serde_json::json!([
        {"op": "set_duration", "activity": "A2", "original": "32h"},
        {"op": "set_duration", "activity": "NOPE", "original": "8h"}
    ]);
    let cases: [(&str, Value, Value, &[&str]); 5] = [
        (&one, lengthen("A2", "32h"), serde_json::json!({}), &[]),
        (
            &unscheduled,
            lengthen("A2", "32h"),
            serde_json::json!({}),
            &[],
        ),
        (
            &one,
            lengthen("A2", "32h"),
            serde_json::json!({"check": true, "data_date": "2026-01-12 08:00"}),
            &["--check", "--data-date", "2026-01-12 08:00"],
        ),
        (
            &one,
            two_edits,
            serde_json::json!({"skip_failed": true}),
            &["--skip-failed"],
        ),
        (&two, lengthen("A1", "16h"), serde_json::json!({}), &[]),
    ];
    for (location, edits, extra, flags) in cases {
        let served = serve(location);
        let mut body = serde_json::json!({"edits": edits, "head": served.head(), "dry_run": true});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let (status, _, preview) = served.post("/api/1/edit", &body);
        assert_eq!(status, 200, "{flags:?}: {preview}");
        let file = directory.path().join("edits.json");
        std::fs::write(&file, edits.to_string()).unwrap();
        let mut command = vec!["edit", location, "--edits", file.to_str().unwrap()];
        command.extend(["--dry-run", "--json"]);
        command.extend(flags);
        let (code, stdout, stderr) = run(&command);
        let mut line: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
        // A skipped edit names where it came from: the request's `edit 2`,
        // the file's `edits.json item 2`. Nothing else may differ.
        let mut preview = preview;
        for report in [&mut preview, &mut line] {
            if let Some(skipped) = report.get_mut("skipped").and_then(Value::as_array_mut) {
                for skipped in skipped {
                    skipped["source"] = Value::Null;
                }
            }
        }
        assert_eq!(
            preview, line,
            "{flags:?}: status {status}, exit {code:?}, {stderr}"
        );
    }
}

/// A dry run doesn't write results into a file it won't keep, but where
/// writing them would fail, it fails as the edit itself does, so a clean
/// preview still means the edit can be recorded.
#[test]
fn a_dry_run_fails_where_the_edit_would() {
    let directory = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(FIXTURE).unwrap();
    // The fixture without PROJECT.scd_end_date, where the finish is written.
    let mut column = None;
    let mut in_project = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if let Some(name) = line.strip_prefix("%T\t") {
                in_project = name == "PROJECT";
            }
            let mut cells: Vec<&str> = line.split('\t').collect();
            if in_project && cells[0] == "%F" {
                column = cells.iter().position(|c| *c == "scd_end_date");
            }
            if in_project && let Some(at) = column.filter(|at| *at < cells.len()) {
                cells.remove(at);
            }
            cells.join("\t")
        })
        .collect();
    assert!(column.is_some());
    let file = directory.path().join("no-finish.xer");
    std::fs::write(&file, lines.join("\r\n") + "\r\n").unwrap();
    let file = file.to_str().unwrap();
    let out = directory.path().join("out.xer");
    for args in [
        vec!["edit", file, "--duration", "A1=16h", "--dry-run", "--json"],
        vec![
            "edit",
            file,
            "--duration",
            "A1=16h",
            "-o",
            out.to_str().unwrap(),
            "--json",
        ],
    ] {
        let (code, stdout, stderr) = run(&args);
        assert_ne!(code, Some(0), "{args:?}: {stdout}");
        assert!(
            stdout.contains("schedule-refused"),
            "{args:?}: {stdout} {stderr}"
        );
    }
}

#[test]
fn a_stale_head_gets_branch_moved() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let seen = served.head();
    let first =
        serde_json::json!({"edits": lengthen("A2", "32h"), "head": seen, "message": "First"});
    assert_eq!(served.post("/api/1/edit", &first).0, 200);
    let late =
        serde_json::json!({"edits": lengthen("A2", "6d"), "head": seen, "message": "Second"});
    let (status, _, refused) = served.post("/api/1/edit", &late);
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error"]["code"], "branch-moved");
    let headless = serde_json::json!({"edits": lengthen("A2", "6d"), "message": "No head"});
    let (status, _, refused) = served.post("/api/1/edit", &headless);
    assert_eq!(status, 400);
    assert_eq!(refused["error"]["code"], "missing-argument");
}

#[test]
fn a_refused_batch_lists_every_problem() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let before = served.head();
    let edits = serde_json::json!([
        {"op": "set_duration", "activity": "NOPE1", "original": "2d"},
        {"op": "set_duration", "activity": "NOPE2", "original": "2d"}
    ]);
    let body = serde_json::json!({"edits": edits, "head": before, "message": "Typos"});
    let (status, moved, refused) = served.post("/api/1/edit", &body);
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["schema"], "totalfloat.error/1");
    assert_eq!(refused["error"]["code"], "edits-refused");
    assert_eq!(refused["error"]["problems"].as_array().unwrap().len(), 2);
    assert_eq!(moved, None, "a refusal reports no head");
    assert_eq!(served.head(), before);
    // One refused edit is still a refused batch, not a missing endpoint.
    let edits = serde_json::json!([{"op": "set_duration", "activity": "NOPE", "original": "2d"}]);
    let body = serde_json::json!({"edits": edits, "head": before, "message": "Typo"});
    let (status, _, refused) = served.post("/api/1/edit", &body);
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["error"]["problems"].as_array().unwrap().len(), 1);
}

#[test]
fn a_proposal_goes_on_the_branch_when_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let before = served.head();
    let body = serde_json::json!({"edits": lengthen("A2", "32h"), "head": before, "message": "Maybe", "propose": true});
    let (status, moved, report) = served.post("/api/1/edit", &body);
    assert_eq!(status, 200, "{report}");
    assert_eq!(
        moved.as_deref(),
        Some(before.as_str()),
        "a proposal leaves the branch"
    );
    let (_, proposals) = served.get("/api/1/history/proposals");
    let name = proposals["proposals"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(name.contains('/'), "{name}");
    let (status, shown) = served.get(&format!("/api/1/history/show/{name}"));
    assert_eq!(status, 200, "{shown}");
    let (status, moved, accepted) = served.post(
        "/api/1/history/accept",
        &serde_json::json!({"proposal": name, "reason": "Agreed"}),
    );
    assert_eq!(status, 200, "{accepted}");
    assert_ne!(moved.unwrap(), before);
    let (_, rows) = served.get("/api/1/rows");
    assert_eq!(by_code(&rows, "original_duration_hours")["A2"], 32.0);
    let (_, proposals) = served.get("/api/1/history/proposals");
    assert_eq!(proposals["proposals"].as_array().unwrap().len(), 0);
}

#[test]
fn export_downloads_what_history_export_writes() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let (status, head, bytes) = served.download("/api/1/history/export?at=main");
    assert_eq!(status, 200, "{head}");
    assert!(
        head.contains("content-disposition: attachment; filename=\"plant-main-"),
        "{head}"
    );
    let written = directory.path().join("exported.xer");
    let (code, _, stderr) = run(&[
        "history",
        "export",
        &store,
        "--at",
        "main",
        "-o",
        written.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(bytes, std::fs::read(&written).unwrap());
    // A changeset, by its ID, is named by its short ID alone.
    let (_, log) = served.get("/api/1/history/log");
    let id = log["changesets"][0]["id"].as_str().unwrap().to_owned();
    let short = log["changesets"][0]["short_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for at in [id.as_str(), short.as_str(), &short.to_ascii_uppercase()] {
        let (status, head, _) = served.download(&format!("/api/1/history/export?at={at}"));
        assert_eq!(status, 200, "{head}");
        assert!(
            head.contains(&format!("filename=\"plant-{short}.xer\"")),
            "{head}"
        );
    }
    // A baseline by its name.
    let (status, _, made) = served.post(
        "/api/1/history/baseline",
        &serde_json::json!({"action": "create", "name": "agreed"}),
    );
    assert_eq!(status, 200, "{made}");
    let (_, head, _) = served.download("/api/1/history/export?at=agreed");
    assert!(
        head.contains(&format!("filename=\"plant-agreed-{short}.xer\"")),
        "{head}"
    );
}

#[test]
fn a_plain_xer_refuses_every_write() {
    let served = serve(FIXTURE);
    let body = serde_json::json!({"edits": lengthen("A2", "32h"), "head": "x", "message": "No"});
    let (status, _, refused) = served.post("/api/1/edit", &body);
    assert_eq!(status, 403, "{refused}");
    assert_eq!(refused["error"]["code"], "read-only");
    assert!(
        refused["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("history init")
    );
    for path in ["revert", "branch", "baseline", "merge", "accept", "reject"] {
        let (status, _, refused) =
            served.post(&format!("/api/1/history/{path}"), &serde_json::json!({}));
        assert_eq!(status, 403, "{path}: {refused}");
        assert_eq!(refused["error"]["code"], "read-only", "{path}");
    }
}

#[test]
fn history_changes_guard_the_branch_shown() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let (code, _, stderr) = run(&["history", "branch", &store, "what-if"]);
    assert_eq!(code, Some(0), "{stderr}");
    let served = serve(&format!("{store}@what-if"));
    // main is checked out, so revert and merge would act on it, not what-if.
    let (status, _, refused) = served.post("/api/1/history/revert", &serde_json::json!({}));
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error"]["code"], "not-checked-out");
    let (status, _, refused) = served.post(
        "/api/1/history/branch",
        &serde_json::json!({"action": "delete", "name": "what-if"}),
    );
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error"]["code"], "branch-shown");
    // A stale head refuses a history change too.
    let (status, _, refused) = served.post(
        "/api/1/history/baseline",
        &serde_json::json!({"action": "create", "name": "x", "head": "stale"}),
    );
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error"]["code"], "branch-moved");
    // Reads follow the branch shown.
    let (_, log) = served.get("/api/1/history/log");
    assert_eq!(log["branch"], "what-if", "{log}");
    // A new branch or baseline starts from what's shown, not from main.
    let edit = serde_json::json!({
        "edits": lengthen("A2", "32h"), "head": served.head(), "message": "What if"
    });
    let (status, shown, answer) = served.post("/api/1/edit", &edit);
    assert_eq!(status, 200, "{answer}");
    let shown = shown.unwrap();
    for (path, name) in [("branch", "from-what-if"), ("baseline", "b1")] {
        let create = serde_json::json!({"action": "create", "name": name});
        let (status, _, answer) = served.post(&format!("/api/1/history/{path}"), &create);
        assert_eq!(status, 200, "{answer}");
        let (status, at) = served.get(&format!("/api/1/history/show/{name}"));
        assert_eq!(status, 200, "{at}");
        assert_eq!(at["id"], shown.as_str(), "{path}: {at}");
    }
}

#[test]
fn a_person_reverts_without_a_reason() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let before = served.head();
    let (status, moved, answer) = served.post("/api/1/history/revert", &serde_json::json!({}));
    assert_eq!(status, 200, "{answer}");
    assert_ne!(moved.unwrap(), before);
    let (_, rows) = served.get("/api/1/rows");
    assert_ne!(
        by_code(&rows, "original_duration_hours")["A1"],
        24.0,
        "the A1 edit is undone"
    );
    let (status, _, refused) =
        served.post("/api/1/history/revert", &serde_json::json!({"reason": 42}));
    assert_eq!(status, 400, "{refused}");
    assert_eq!(refused["error"]["code"], "bad-body");
}

#[test]
fn a_history_change_sees_a_move_made_outside_the_server() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let seen = served.head();
    let (code, _, stderr) = run(&["edit", &store, "--duration", "A2=48h", "-m", "From the CLI"]);
    assert_eq!(code, Some(0), "{stderr}");
    let (status, _, refused) =
        served.post("/api/1/history/revert", &serde_json::json!({"head": seen}));
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error"]["code"], "branch-moved");
    let (code, log, stderr) = run(&["history", "log", &store]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        log.contains("From the CLI"),
        "the outside change stays: {log}"
    );
    // The refusal shows the branch as it is now, so a reload can carry on.
    let now = served.head();
    assert_ne!(now, seen, "the view caught up");
    let (status, moved, answer) =
        served.post("/api/1/history/revert", &serde_json::json!({"head": now}));
    assert_eq!(status, 200, "{answer}");
    assert_ne!(moved.unwrap(), now);
    // So does an edit refused for the same reason.
    let (code, _, stderr) = run(&["edit", &store, "--duration", "A2=40h", "-m", "Again"]);
    assert_eq!(code, Some(0), "{stderr}");
    let stale = served.head();
    let edit =
        serde_json::json!({"edits": lengthen("A2", "32h"), "head": stale, "message": "Late"});
    assert_eq!(served.post("/api/1/edit", &edit).0, 409);
    assert_ne!(served.head(), stale);
}

#[test]
fn the_page_hears_of_a_change_made_elsewhere_within_two_seconds() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let mut feed = served.feed();
    let start = Instant::now();
    let (code, _, stderr) = run(&["edit", &store, "--duration", "A2=32h", "-m", "From the CLI"]);
    assert_eq!(code, Some(0), "{stderr}");
    let deadline = start + Duration::from_secs(2);
    let changeset = feed.next("changeset", deadline);
    assert_eq!(changeset["schema"], "totalfloat.event/1");
    assert_eq!(changeset["changeset"]["message"], "From the CLI");
    assert_eq!(changeset["branch"], "main");
    let view = feed.next("view", deadline);
    assert_eq!(view["head"], changeset["changeset"]["id"]);
    assert_eq!(served.head(), view["head"].as_str().unwrap());

    // An agent's proposal, from another process, arrives too.
    let start = Instant::now();
    let (code, _, stderr) = run_with(
        &[
            "edit",
            &store,
            "--duration",
            "A2=40h",
            "-m",
            "Agent idea",
            "--propose",
        ],
        &[
            ("TOTALFLOAT_AUTHOR", "planner-bot"),
            ("TOTALFLOAT_AUTHOR_KIND", "agent"),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let proposal = feed.next("proposal", start + Duration::from_secs(2));
    assert_eq!(proposal["change"], "new");
    assert_eq!(proposal["branch"], "main");
    assert_eq!(proposal["changeset"]["author_kind"], "agent");
    assert_eq!(proposal["changeset"]["state"], "proposed");
}

#[test]
fn the_page_hears_of_its_own_changes_the_same_way() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let mut feed = served.feed();
    let start = Instant::now();
    let body = serde_json::json!({
        "edits": lengthen("A2", "32h"), "head": served.head(), "message": "From the page"
    });
    let (status, head, answer) = served.post("/api/1/edit", &body);
    assert_eq!(status, 200, "{answer}");
    let deadline = start + Duration::from_secs(2);
    let changeset = feed.next("changeset", deadline);
    assert_eq!(changeset["changeset"]["author_kind"], "human");
    let view = feed.next("view", deadline);
    assert_eq!(view["head"], head.unwrap().as_str());
}

/// `GET /` gives the app built into this binary, or, in a build without
/// `ui/dist/`, the page saying how to build it.
#[test]
fn the_page_is_the_built_app_or_says_how_to_build_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let request = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\nConnection: close\r\n\r\n",
        served.port, served.cookie
    );
    let (status, head, body) = exchange(served.port, &request);
    assert_eq!(status, 200, "{head}");
    let page = String::from_utf8(body).unwrap();
    if cfg!(totalfloat_ui) {
        assert!(page.contains("<div id=\"root\">"), "{page}");
        let request = format!(
            "GET /favicon.svg HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            served.port
        );
        let (status, head, _) = exchange(served.port, &request);
        assert_eq!(status, 200, "{head}");
        assert!(head.contains("image/svg+xml"), "{head}");
    } else {
        assert!(page.contains("cargo xtask ui"), "{page}");
    }
}

/// The fields ui/src/api.ts reads from the summary, with their JSON types.
/// Change both together.
#[test]
fn the_summary_has_what_the_page_reads() {
    let text = |v: &Value| v.is_string();
    let text_or_null = |v: &Value| v.is_string() || v.is_null();
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let served = serve(&store);
    let (status, summary) = served.get("/api/1/summary");
    assert_eq!(status, 200, "{summary}");
    assert!(text(&summary["location"]), "{summary}");
    assert!(text_or_null(&summary["branch"]) && text_or_null(&summary["head"]));
    assert!(summary["writable"].is_boolean() && summary["calculated"].is_boolean());
    let project = &summary["inspect"]["projects"][0];
    assert!(
        project["id"].is_u64() && project["activities"].is_u64(),
        "{project}"
    );
    assert!(text(&project["short_name"]), "{project}");
    for key in ["name", "data_date", "finish"] {
        assert!(text_or_null(&project[key]), "{key}: {project}");
    }

    // A schedule that can't be calculated says why, as an error object.
    let looped = directory.path().join("loop.xer");
    let fixture = std::fs::read_to_string(FIXTURE).unwrap();
    let link = "%R\t1\t2\t1\t1\t1\tPR_FS\t1\t\t\t\t";
    assert!(fixture.contains(link));
    let eol = if fixture.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let back = format!("{link}{eol}%R\t2\t1\t2\t1\t1\tPR_FS\t0\t\t\t\t");
    std::fs::write(&looped, fixture.replace(link, &back)).unwrap();
    let served = serve(looped.to_str().unwrap());
    let (_, summary) = served.get("/api/1/summary");
    assert_eq!(summary["calculated"], false, "{summary}");
    let refused = &summary["calculation_refused"];
    for key in ["code", "message", "hint"] {
        assert!(text(&refused[key]), "{key}: {refused}");
    }
}
