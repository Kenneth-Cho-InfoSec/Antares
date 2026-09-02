# Antares Stylo integration

This directory vendors the Stylo source used by Antares so that engine builds are reproducible and the CSS compatibility changes remain reviewable.

- Upstream: <https://github.com/servo/stylo>
- Base revision: `b3e6425100df710c7f30125e27a8fb897a709c5e`
- Licence: Mozilla Public License 2.0, as recorded in the upstream source files and `LICENSE`

Antares changes from the base revision:

- Expose the standards-defined `text` value of `background-clip` to Servo builds.
- Expose the compatibility-standard `-webkit-text-fill-color` property to Servo builds.
- Mark changes to text fill colour as requiring repainting.

The corresponding paint implementation and regression coverage live in the main Antares tree. When updating Stylo, start from a clean copy of the new pinned revision, reapply these small changes, run the style and layout checks, then run the relevant Web Platform Tests before updating `Cargo.lock`.
