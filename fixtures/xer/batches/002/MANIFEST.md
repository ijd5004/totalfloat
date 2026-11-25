# Batch manifest

Authored synthetic inputs, Apache-2.0, with no stored results. Expected results come from TotalFloat's written rules (docs/rules/).

| File | Input spec SHA-256 | XER SHA-256 | Description | Results |
|---|---|---|---|---|
| `cal-edit-after-schedule.xer` | `5ea6a8bf7656a6e499dd80e14af9c8388fc27dfbfeffd7295b2e7fbcc82ba180` | `d878bc21e327bf4eb247a487cfdfca3b4a0771622b1dc09ba4e7fe6e9843ea28` | Seven-day calendar with Friday exceptions for the TF-029 question (docs/FIXTURE-SPECS.md#batch-002); no stored results | none stored |
| `span-links-and-open-starts.xer` | `7ba36aab7d2a6c0771d1666c64e2f600c61853693f88107f3036623105933fae` | `074cec02912d1bda6ecfbbd0f08ab5777cfba5309b5c8927cd66739cd2c4eced` | Level of effort with no start link, a level of effort linked to another, and spans under a late deadline (TF-027); no stored results | none stored |
| `zero-duration-after-weekend.xer` | `2cd828bc5f9538dfcb48c0e72a2f7a0f7d2e125db52e5de275d30e4266656dbb` | `68d4275872bfe32805158e3e59934c78990d4c46cca9fae03e1dd9cc556520d2` | Zero-duration tasks that follow a Friday finish and a midweek finish (TF-014); no stored results | none stored |
