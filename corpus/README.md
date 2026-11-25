# The corpus

Real schedule files posted publicly, which TotalFloat is tested on. [`manifest.toml`](manifest.toml) lists every one. The files aren't in this repository, because most carry no license to redistribute them; each stays under the terms of the place it's posted.

## Getting the files

```sh
export TOTALFLOAT_CORPUS=~/totalfloat-corpus   # any folder outside this repository
cargo xtask corpus fetch
```

`fetch` downloads each listed file into `$TOTALFLOAT_CORPUS/files/<id>/<filename>` and keeps it only if its SHA-256 matches the manifest. A file that has moved or changed is reported and left out. Run it again at any time; it downloads only what's missing. `cargo xtask corpus check` verifies the folder against the manifest.

With `TOTALFLOAT_CORPUS` set, `cargo test` runs the corpus tests as well.

## What each entry records

| Field | Meaning |
|---|---|
| `id` | `C-` and four digits. IDs are never reused, so gaps are files that were removed. |
| `filename` | The file's name as downloaded. |
| `source` | A short name for where it came from. |
| `url` | Where it's posted. |
| `license` | The license stated where it's posted, or `none stated`. |
| `obtained` | The date we downloaded it. |
| `p6_version` | The version an XER file's header line gives. |
| `sha256` | The SHA-256 of the file as downloaded. |
| `stale` | The dates stored in the file don't follow from its own inputs, so rules don't use it as evidence. |
| `tags` | `p6-export`: an XER export of a real project. `msproject-xml`: an MS Project XML file. `synthetic`: a file a tool generated. `codec-only`: another tool's test or demo file, used for format facts and reading and writing tests only. `fresh-reference`: the export TotalFloat's new-file layout follows. `series-*`: exports of one project. |

## What it may hold

- Files posted publicly, at a link anyone can download from.
- Never a vendor's own sample or demo projects.
- Other tools' test and demo files only as `codec-only`.

What TotalFloat's rules say the files show is in [docs/rules](../docs/rules/README.md).

If you posted a file listed here and want it removed, open an issue and we'll take it off the list.
