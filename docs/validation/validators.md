# File validators

`cargo xtask validate` checks a file TotalFloat wrote with software we didn't write. It checks the files themselves; the table says what each check shows.

| Command | Check | What a pass shows |
|---|---|---|
| `cargo xtask validate xer FILE` | XER Tools 2.2b's `/INTEGRITY` checks, under Wine | The file passes an independent XER viewer's integrity checks with no errors, and every check ran. Warnings are reported. |
| | MPXJ 16.11.0 reads it | An independent open-source reader parses it. Counts are printed. |
| `cargo xtask validate msproject FILE` | `xmllint` against Microsoft's MS Project XML schema | The file is valid against the schema Microsoft published for the format. |
| | MPXJ 16.11.0 and TotalFloat's reader both read it | Both readers see the same numbers of activities, links, resources, assignments and calendars, once the items TotalFloat's import notes say it left out on purpose (inactive tasks, links on summary tasks or to left-out tasks, unused resources) are counted back in. The output names those items. |

Each command prints one line per check and exits non-zero if any check failed. A check whose tool isn't installed says `skipped` and doesn't fail, so the commands run anywhere; a skipped check proves nothing.

XER Tools writes its summary only when it closes, so the XER check gives it 30 seconds plus a second per 100 KB of file (at most 300 seconds), closes it, and reads the session it added to its log; a copy of that session goes to `logs/` in the cache. A summary showing fewer checks completed than planned fails. Runs share one Wine prefix, so run one XER check at a time.

Neither MPXJ nor the schema stands in for Microsoft Project. MPXJ is lenient and reads files Project might refuse. The schema is strict about element order and required elements. It is the 2007 schema, so elements added to the format later are reported as unexpected.

## Why these tools

- **XER Tools 2.2b** (David Hoyle) is the XER check; see [xertools-2.2b.md](xertools-2.2b.md).
- **GanttProject** was tried for MS Project XML first. Its command-line export works on its own `.gan` files, but given MS Project XML it opens an empty project (3.3.3323, 2026-11-17), and it imports MS Project XML through MPXJ anyway. It stays the tool for differential scheduling tests.
- **MPXJ** is the MS Project XML and XER reader most open-source schedulers use. It's LGPL-2.1, so we run it as a separate program.
- **The schema** is assembled from the sections Microsoft publishes in [MicrosoftDocs/office-developer-msproject-xml-docs](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs) (documentation CC BY 4.0, code MIT), at a pinned commit, each page checked by SHA-256. It stays in the cache and is never committed. Its target namespace is changed from `http://schemas.microsoft.com/project/2007` to `http://schemas.microsoft.com/project`, the one files use.

## Installing

Everything lives under `~/.cache/totalfloat/` (or `$TOTALFLOAT_CACHE`), nothing system-wide.

| Path | What | Pin |
|---|---|---|
| `runtime/wine-11.20/` and `external/xertools/2.2b/` | Wine and XER Tools 2.2b with its prefix | see [xertools-2.2b.md](xertools-2.2b.md) |
| `runtime/liberica-jdk-21.0.13/` | BellSoft Liberica Full JDK 21.0.13.0.3+5, Linux x64 (`bellsoft-jdk21.0.13.0.3+5-linux-amd64-full.tar.gz`) | SHA-1 `451cc597bd5ddbb597fe4dab08d64e88eedeb52b`, as BellSoft publishes it |
| `external/mpxj/mpxj/` | MPXJ 16.11.0 (`mpxj-16.11.0.zip` from the GitHub release), unzipped | SHA-256 `765f0f5e459f27b7f12511bea682f13937d92ddd37d5422a693d746c6a490273` |
| `external/mspdi/mspdi.xsd` | The assembled schema | `cargo xtask validate fetch-schema` (needs `curl`) |

`xmllint` comes with libxml2 (`libxml2` on Arch, `libxml2-utils` on Debian and Ubuntu). A JDK, not a JRE, is needed: the MPXJ step runs `xtask/src/validate/MpxjSummary.java` as a single-file program.

```sh
B=~/.cache/totalfloat
mkdir -p $B/runtime/liberica-jdk-21.0.13 $B/external/mpxj
curl -sSLO https://github.com/bell-sw/Liberica/releases/download/21.0.13.0.3+5/bellsoft-jdk21.0.13.0.3+5-linux-amd64-full.tar.gz
tar -xzf bellsoft-jdk21.0.13.0.3+5-linux-amd64-full.tar.gz -C $B/runtime/liberica-jdk-21.0.13 --strip-components=1
curl -sSLO https://github.com/joniles/mpxj/releases/download/v16.11.0/mpxj-16.11.0.zip
unzip -q mpxj-16.11.0.zip -d $B/external/mpxj
cargo xtask validate fetch-schema
```

Check each download against its pin before unpacking it.

## Results

| File | Checked | Result |
|---|---|---|
| `fixtures/xer/fresh-writer.xer` | 2026-11-24 | XER Tools: "182 of 172" checks (the viewer's own count, see [xertools-2.2b.md](xertools-2.2b.md)), no warnings or errors. MPXJ: 2 activities, 1 link, 1 resource, 1 assignment, 1 calendar, 1 summary (the WBS node). |
| A truncated copy of it (first 600 bytes) | 2026-11-24 | XER Tools: 2 errors. MPXJ: not recognised. |
| `totalfloat export --to msproject` of the fixture | 2026-11-24 | Schema: valid. MPXJ read-back: both readers see 2 activities, 1 link, 1 resource, 1 assignment, 1 calendar. |
| A truncated copy of that export (first 1,500 bytes) | 2026-11-24 | Schema: parser error. MPXJ: not recognised. |
| GanttProject 3.3.3323's MS Project XML export of its house-building sample | 2026-11-17 | Schema: fails on element order. Read-back: passes, 17 links on both sides, 3 of them links on summary tasks that TotalFloat leaves out on purpose and reports in its import notes. |
| A minimal file with only the required elements | 2026-11-17 | Schema: valid. MPXJ: not recognised, as the file has no tasks. |
