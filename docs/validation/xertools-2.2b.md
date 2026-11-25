# XER Tools 2.2b checks

XER Tools 2.2b, by David Hoyle, is an independent XER viewer with file-integrity checks. TotalFloat uses it to check the XER files it writes.

## Setup

- `XERTools64.exe` 2.2b, the official portable build, SHA-256 `c62132b0a7877b9578a19e38a37107e2e974e19e52045aba1a7a7ed32b61ad80`.
- Run under Wine 11.20 on Linux x86-64, in an isolated prefix with default settings. No application code or binaries are in this repository.
- Each file is opened with the `/INTEGRITY` switch, default checks, and no cleaning or check exclusions.

`cargo xtask validate xer FILE` runs the check headless and copies the session log to `~/.cache/totalfloat/logs/`; [validators.md](validators.md) says how to install it.

## Reproduce by hand

```powershell
& 'C:\Tools\XERTools\XERTools64.exe' /INTEGRITY 'C:\path\file.xer'
```

Check the file tab, the completion banner and the error and warning summary, then expand the report and copy it (Ctrl+A, Ctrl+C). Clear the clipboard before each file so a stale report can't be reused, and don't treat the process's exit status as the result.

The viewer's completion banner sometimes counts more checks completed than planned (`182 of 172`). Banners are recorded as the viewer shows them.

**Negative control:** a copy of `fixtures/xer/anonymize-clean.xer` with its one `TASKPRED.pred_task_id` changed to `999999` fails two checks: the predecessor isn't in the task index, and its project ID can't be resolved. So the procedure catches a broken relationship reference.

## Latest run (2026-11-14)

Every committed XER under `fixtures/xer/` at the hashes below, and two `edit` outputs, were loaded one at a time. Each file's tab and banner were checked against its copied report. Screenshots and reports stay outside the repository. A changed file needs checking again; `curves.xer` and `monitoring.xer` were added after this run.

**Result:** no file had an error, and every writer output passed with no warnings. Three files didn't pass cleanly:

- `spreads.xer` has one warning: `TASKRSRC.curv_id` `9` isn't in the curve index. Corpus XER files use that value to mark a hand-typed spread, with no curve 9 row ([TF-040](../rules/TF-040-typed-spreads.md)), and the viewer gives a corpus export carrying the marker the same warning.
- `model-mapping.xer` and `two-projects.xer` are hand-written mapping inputs that carry only the columns their tests read. They aren't writer output. The viewer reported 25 missing-field warnings and stopped after the `RSRC` table.

| File (under `fixtures/xer/`) | Viewer completion counter | Errors | Warnings | Outcome | SHA-256 |
|---|---|---|---|---|---|
| `anonymize-clean.expected.xer` | 107 of 107 | 0 | 0 | pass | `c56d778a2dcaf9c93f35402a1285a05d07e32392c79f83a4362e4e7362f06c94` |
| `anonymize-clean.xer` | 107 of 107 | 0 | 0 | pass | `700c695b43ae74dd923800c0119d25c3fad74943f060c1d72a98fb8c9f8bd7ed` |
| `expenses.xer` | 359 of 309 | 0 | 0 | pass | `7e6fb93e0f59967eb5ef9ae4f07231d12dbf2cd2ff1b56c0b03048f3191d3707` |
| `fresh-writer.xer` | 182 of 172 | 0 | 0 | pass | `8f7df081f380a7c1913aed2b9bcf184a7b4f1b88cf84883e5b94e01047e03e97` |
| `midnight-finish.xer` | 115 of 115 | 0 | 0 | pass | `23031bef7d9de100c1a79963f186588ba3dfd212c09bf5113ff17a059f2625db` |
| `model-mapping.xer` | did not complete (`????`) | 0 | 25 so far | stopped after `RSRC` | `ad76918da26513de4a756e7429074d92f9a95ce97231a77373f9a8be18765880` |
| `notebooks.xer` | 345 of 295 | 0 | 0 | pass | `e0806cdb03ac3b6334cdc8ec4d350daec713fc8b3c856c481212524f73f79380` |
| `obs.xer` | 326 of 276 | 0 | 0 | pass | `3b07888be82bad97b31c591c22d80badbdc9a63c4734e18d0706d36f84b0928b` |
| `project-codes.xer` | 344 of 294 | 0 | 0 | pass | `719c45ff3dda0f5dc409d2eb116ac434579d5dcbc03b88646f20ae5d5468e520` |
| `roles.xer` | 356 of 306 | 0 | 0 | pass | `bc49bef38e604c933cf129ec871bd3841478ac39e71053b298a1f99206b8955f` |
| `spreads.xer` | 316 of 266 | 0 | 1 | expected warning | `f4b616bd2f6042e3aaa07b21e073b898a2dda8db0fb6604b654dc581d6f691d2` |
| `steps.xer` | 334 of 284 | 0 | 0 | pass | `3a8c335ea01f3729b85a9569a5e68ce5c11a209fd79f09263bd090f53bdc96d7` |
| `two-projects.xer` | did not complete (`????`) | 0 | 25 so far | stopped after `RSRC` | `096c9706e46634f512ab2d6794651bd3dc16f1e398c89f4f7b71344160506f98` |
| `usage.xer` | 351 of 301 | 0 | 0 | pass | `7f9ab5d4110bc9da3075e9c1709aa86c27ce80be40530ad889a70c8925a8f2dc` |
| `validation/anonymize-metadata/anonymized.xer` | 202 of 192 | 0 | 0 | pass | `e9245ee0ff9eee9fa436a0549c826a5816c9152b2fb7462aa33df215ad630222` |
| `validation/anonymize-metadata/source.xer` | 202 of 192 | 0 | 0 | pass | `bdcbbcc6050c51ec5c712533636710d9f5adb5bd855d7249e4c2053f6e6bfade` |
| `validation/patches/baseline.xer` | 182 of 172 | 0 | 0 | pass | `8f7df081f380a7c1913aed2b9bcf184a7b4f1b88cf84883e5b94e01047e03e97` |
| `validation/patches/delete.xer` | 127 of 117 | 0 | 0 | pass | `8dfdc91f9a4d0e0d6fe37baeec159c36345e0b5ca5203652c0809d1187f38c11` |
| `validation/patches/duration.xer` | 182 of 172 | 0 | 0 | pass | `3c1f38c6bf64299db5a23dc40a8fd6df8cb7e75e0e53e7de61b9a68f4a616dc9` |
| `validation/patches/insert.xer` | 210 of 200 | 0 | 0 | pass | `42379ce2d5b9a7599c9f096998f2dcaa14ba13eda88ee04f81d5314dae937fe2` |
| `validation/patches/lag.xer` | 182 of 172 | 0 | 0 | pass | `0fb30078e3c174cb4d3eb79356dac432d700f5883a23009412e55c1c616bd310` |
| `validation/patches/precision-duration.xer` | 182 of 172 | 0 | 0 | pass | `60f73781831f6fc62b70b0bb9cf1166bb15bb9c7c06fcbff82ba8f186a37ffef` |
| `validation/patches/precision-lag.xer` | 182 of 172 | 0 | 0 | pass | `c1d27cb08519667037629afc2ae1b43e557da95773b3aa43a04bf5d85b476f9f` |
| `validation/patches/project-date.xer` | 182 of 172 | 0 | 0 | pass | `2cc861607aebdf6d09a7518ed09b50070ff3711935796f02e0d676eb84f017f5` |

All 23 files in `batches/001/` and all 3 in `batches/002/` pass with no errors or warnings. Their per-file hashes are in each batch's `MANIFEST.md`.

### Edit outputs

Neither output is committed. Both regenerate byte for byte from these commands:

```sh
F=fixtures/xer/fresh-writer.xer
totalfloat edit $F -o edit-add-delete.xer \
  --add "TF-003;Synthetic added work;TF-FRESH;3d" --link "TF-003>TF-002" --delete TF-001
awk '/^%T/{t=$2; sub(/\r$/,"",t)} /^%E/{t=""} !(t=="RSRC"||t=="RSRCRATE"||t=="TASKRSRC")' $F > no-resources.xer
totalfloat edit no-resources.xer -o edit-new-tables.xer \
  --add-resource "CREW;Synthetic crew;labor" --assign "TF-001=CREW:16"
```

| Output | What it tests | Viewer completion counter | Errors | Warnings | SHA-256 |
|---|---|---|---|---|---|
| `edit-add-delete.xer` | Adds an activity and deletes another, with its link, assignment, code and UDF value | 155 of 145 | 0 | 0 | `f88217e3fdec3c82f768391c8bf198ca9e1e684c83ce8c4db6deb00bba2f018a` |
| `no-resources.xer` | `fresh-writer.xer` without `RSRC`, `RSRCRATE` and `TASKRSRC`, the control for the next row | 150 of 140 | 0 | 0 | `bfe5ebd0eecd70286eff21580c3edf45de67c6929f800ae72b20e1e9bf765ff5` |
| `edit-new-tables.xer` | Adds a resource and an assignment, so the patch writer appends `RSRC` and `TASKRSRC` after `TASK` and every other table | 182 of 172 | 0 | 0 | `63ef38c6b22f60c7ca84433c2d517d32fc3355b77315fe4a97dda11aa57096d0` |

**Table order:** XER Tools accepts `RSRC` and `TASKRSRC` at the end of the file.
Its report validates `TASKRSRC.rsrc_id` against `RSRC` and `TASKRSRC.task_id`
against `TASK` with no warning.
