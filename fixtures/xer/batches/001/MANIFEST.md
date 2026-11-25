# Batch manifest

Authored synthetic inputs, Apache-2.0, with no stored results. Expected results come from TotalFloat's written rules (docs/rules/).

| File | Input spec SHA-256 | XER SHA-256 | Description | Results |
|---|---|---|---|---|
| `c-alap.xer` | `408609bfe974143a4f0d02854036b6b70a5dc1ec6008199c2fb6541753032bf3` | `8ff6f04000ab43827e694f889c2c82e619b4a833425377b022d9150729c3089d` | Constraint as_late_as_possible; float type start | none stored |
| `c-finish-after.xer` | `590e982e0e3a2668a8929ce644eb4e80a8115711fc57132cea3ed7a29e5c31e8` | `65fc688abbb78e65a5255c0d1fdf7974e9108bfcbfc429cffa80094e8ddb8203` | Constraint finish_on_or_after; float type finish | none stored |
| `c-finish-before.xer` | `0ae739be45ee35a4c14b4b6631a88477f65f9e760c6855122749d38af77e77c4` | `f3c0f12d1d0cda70e1dee25b45f169fb16650d19277ff1b098cef153f8cc6225` | Constraint finish_on_or_before; float type smallest | none stored |
| `c-finish-on.xer` | `5f4e68790c89194ab9b1a765c7f6881001de60bae950386064bbce0047400f61` | `3176fc4009515e3b3ce84111d7280b700cf07a27693428fdad3591b50459c5f9` | Constraint finish_on; float type start | none stored |
| `c-mandatory-finish.xer` | `4834b8a312d5b804cceb9693a0cdcf90dbb5fe165fdbbda32dec2f9e0ccb19da` | `2dcfd1a85c0bcfd64bd09dd55aa39a79a73d6ad364ace00f7ec5b6b88bf9dfda` | Constraint mandatory_finish; float type smallest | none stored |
| `c-mandatory-start.xer` | `3fbd9f559827bb5740c44f782175f9d2390d7ceeb09cbea7e6e4225f935af839` | `62ae5dda56e88e7431d913f2e77d5ea32244a13459df7b5d78cd460612b69a4f` | Constraint mandatory_start; float type finish | none stored |
| `c-start-after.xer` | `925b6f8068a6de1947c6e1e78b28839497e53bd2ecf201b213e3506206184fba` | `cf709c8601b592703d5c3da84a279b1701a5b67b4d6301c02e716122b8d3d7db` | Constraint start_on_or_after; float type finish | none stored |
| `c-start-before.xer` | `7c433f642ac15845b7c8b522176d519a8cdf28d67926bde40b08b5298def8ce1` | `2370d4ac86304ad03a56d43e0f52b9729c8722931762056878858e2e6930452a` | Constraint start_on_or_before; float type smallest | none stored |
| `c-start-on.xer` | `455fdc1dc332fa4c42f16319dc9feaaf3970ae29a01ead3f84dc83703beb16dc` | `67360a52f3bd9078a6382190bbf639c8e86f43510f1b5ab1387ff80d728b5b71` | Constraint start_on; float type start | none stored |
| `loe.xer` | `d9dff96de9afff5dac44aff724ce784a74f639ddd06f32f1462a81ba591131eb` | `805f968783f45e37988bbd162d3e2362126a95b47561774c7f6ea2b57eeaa19e` | LOE spanning SS predecessor and FF successor | none stored |
| `milestones.xer` | `2496189641d79f66c1bf8b86a5d69d3857cb86fe316d95f26551ae38248dc0f5` | `c58c9e4185d3cb36c193a712044120c66d771a65b48139459c7ed4a2aa0b6ce0` | Start and finish milestones connected with FS | none stored |
| `oos-override.xer` | `ba4d580b486011d293e9d3d3a2da9daba5227297b489c4d9080151f2908d0d9c` | `f3f1d8d2050d9138454530b814a74eea8ba2e3ce09cc76f7033c82846d1659bb` | Out-of-sequence successor has actual start while predecessor is not started; progress_override | none stored |
| `oos-retained.xer` | `eb9ea4b02a8c01ec4d2678385c0cafc080a86580592b9dffed6257708c17beb8` | `25015100d19dffbe7c6bf8d9d498b33d21733ffb96f052e9bca2dae267bd63e4` | Out-of-sequence successor has actual start while predecessor is not started; retained_logic | none stored |
| `open-ends.xer` | `8ff62f510814516d039627318d3cb558ed265946c6d620249e56f12fb5e5f5ee` | `a4cc39dc76af2b75537e1cebb4902d7b2bf0d6df48c9818e343bde0cf8b7b1e1` | Three independent open-ended activities | none stored |
| `rel-ff-negative.xer` | `9339c073e7dbf5c9db6c58cc1eed25caa44c8e5394f59f8b67db206cbd39e1ca` | `1d08672bf971c1af3869442a55125289a5cd77f3cf162532a2d450da20e20e4c` | FF with negative 60-minute lag; lag calendar project | none stored |
| `rel-ff-positive.xer` | `2f076be702301fa78ecf4945f7c4c9940f683adea32caf765f8f57a736a66ad5` | `69aa9a7bb8b2c2f4c606df56407ba18ce347555fb9fd5ff70a18b5273fd53c18` | FF with positive 60-minute lag; lag calendar project | none stored |
| `rel-fs-negative.xer` | `7a09bea72d92737eb3f28c9c6177775d1729f8a5b5099e8c6601aa36234704e0` | `7085aa0ac8911a0de76d70feca77aa4a6cd75c4c752d4c32e3a1fd038de996b6` | FS with negative 60-minute lag; lag calendar predecessor | none stored |
| `rel-fs-positive.xer` | `ec9da90cb6608460dc5b30205738173ef54a8e3c1411d56e08f9d80a55d641b6` | `d6bbd0f3d08461ec9f26878c876f779672d639a4dcd301e10b6d834cb845dd03` | FS with positive 60-minute lag; lag calendar predecessor | none stored |
| `rel-sf-negative.xer` | `3de987f42a9f440fa2611e9383bf46754c39dcb6159eb553f790a9fd1c3bcf5b` | `076f01d9002231194d6cb53b4a5d12d81216fbecca00d539df84d80c0c0b4809` | SF with negative 60-minute lag; lag calendar 24_hour | none stored |
| `rel-sf-positive.xer` | `746b89d337946f068711d087fb239a94aa5740e8393917bdb6dec6abcc54db5f` | `04dbeefa9b236c5080ab3c332d087e0bc1b0960720d3eda13e197a2fcbab8d17` | SF with positive 60-minute lag; lag calendar 24_hour | none stored |
| `rel-ss-negative.xer` | `baea95495917c8b1b69373517b1496cf022d9df1c3e3be988a0ff73f3e25c748` | `c78bb01b47395e7100f6134bf124386128993dbcdf85dec092a18d21f1f17c03` | SS with negative 60-minute lag; lag calendar successor | none stored |
| `rel-ss-positive.xer` | `1f851bf7934b42b9f30b0bcc3599fff584540c65a78c335c201f804d7350436c` | `d4a95d3cad9f0efc6018add1fe8e9d5e36da78ea955539c846f917bdb18fbcf7` | SS with positive 60-minute lag; lag calendar successor | none stored |
| `wbs-summary.xer` | `06872736747cf0ef0d4f95fab543cd99e6441f0c6f5c00c8893739c209b2bf79` | `62674f335864b3d0401079848181e72e5a231171e95e56434a935619e828882c` | WBS summary with ordinary activities in the same WBS | none stored |
