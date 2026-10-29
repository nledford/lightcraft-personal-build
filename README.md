# lightcraft-personal-build

Personal builds of [LightCraft](https://github.com/storytold/lightcraft) that include my open pull requests. Not for review, and never a source of pull requests: this repository is not a fork.

- `main` holds only this README and the workflow `.github/workflows/build.yml`.
- Source branches: `integration` (the build I run) and `candidate-<name>` (an integration rebuild to compare against it), pushed by my local tooling and force-pushed on every rebuild.
- Each run builds `LightCraft.app` for macOS arm64 (ad-hoc signed, not notarized) with storytold's `packaging/macos/package.sh`, and runs every `cargo xtask ci` step on Linux. Artifacts are kept 30 days.

LightCraft is MIT OR Apache-2.0 (see `LICENSE-MIT`, `LICENSE-APACHE` and `NOTICE` on each source branch); the ArtCraft name and logos in `docs/brand/` are trademarks under their own terms.

