# Patch validation fixtures

Authored synthetic inputs, Apache-2.0, with no stored results.
Source: `fixtures/xer/fresh-writer.xer`, SHA-256 `8f7df081f380a7c1913aed2b9bcf184a7b4f1b88cf84883e5b94e01047e03e97`. Every patch starts independently from that authored source.

| File | Operation | SHA-256 | Results |
|---|---|---|---|
| `baseline.xer` | Byte-identical copy of the authored source | `8f7df081f380a7c1913aed2b9bcf184a7b4f1b88cf84883e5b94e01047e03e97` | no stored results |
| `duration.xer` | Set task 1 original/remaining duration to an explicit 600 minutes | `3c1f38c6bf64299db5a23dc40a8fd6df8cb7e75e0e53e7de61b9a68f4a616dc9` | no stored results |
| `lag.xer` | Set relationship 1 lag to an explicit 60 minutes | `0fb30078e3c174cb4d3eb79356dac432d700f5883a23009412e55c1c616bd310` | no stored results |
| `precision-duration.xer` | Set task 1 durations to 469 minutes (six-decimal export precision) | `60f73781831f6fc62b70b0bb9cf1166bb15bb9c7c06fcbff82ba8f186a37ffef` | no stored results |
| `precision-lag.xer` | Set relationship 1 lag to negative 20 minutes (six-decimal export precision) | `c1d27cb08519667037629afc2ae1b43e557da95773b3aa43a04bf5d85b476f9f` | no stored results |
| `project-date.xer` | Set the project data date to an explicit 2026-01-13 08:00 input | `2cc861607aebdf6d09a7518ed09b50070ff3711935796f02e0d676eb84f017f5` | no stored results |
| `insert.xer` | Append task 3 and FS relationship 2 using new table-local IDs | `42379ce2d5b9a7599c9f096998f2dcaa14ba13eda88ee04f81d5314dae937fe2` | no stored results |
| `delete.xer` | Delete task 1; remove its relationship, assignment, activity-code link and UDF value | `8dfdc91f9a4d0e0d6fe37baeec159c36345e0b5ca5203652c0809d1187f38c11` | no stored results |
