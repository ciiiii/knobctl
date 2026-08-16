# Releasing

Releases are driven by [release-please](https://github.com/googleapis/release-please)
in manifest mode (`release-please-config.json` + `.release-please-manifest.json`),
the same setup as `agent-ide-bridge`. You never tag by hand.

## The flow

1. **Commit with [conventional commits](https://www.conventionalcommits.org)** on
   `master`. The prefix decides the version bump:

   | Commit                  | Bump  |
   | ----------------------- | ----- |
   | `fix: …`                | patch |
   | `feat: …`               | minor |
   | `feat!: …` / `BREAKING CHANGE:` | major |
   | `docs: …`, `chore: …`, `refactor: …` | none |

2. **release-please opens a release PR** that bumps `version` in `Cargo.toml`
   (and `Cargo.lock`, via the `rust` release type) and updates `CHANGELOG.md`.
   It keeps that PR up to date as more commits land.

3. **Merge the release PR.** That creates the `vX.Y.Z` tag and the GitHub release
   from the changelog.

4. The `attach-binaries` job then builds all four targets and uploads the archives
   onto that release with `gh release upload --clobber`.

Because the version only ever changes through the release PR, the tag and
`Cargo.toml` can't drift.

## What gets attached

One `.tar.gz` per target, each with a `.sha256` next to it:

| Target                      | Built on           |
| --------------------------- | ------------------ |
| `aarch64-apple-darwin`      | `macos-latest`     |
| `x86_64-apple-darwin`       | `macos-latest`     |
| `x86_64-unknown-linux-gnu`  | `ubuntu-latest`    |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` |

Each archive contains the `knobctl` binary, `README.md`, `LICENSE`, and
`mapping.example.yaml`.

`make dist` produces the identical archive for your host triple in `dist/` — use it
to check packaging before merging a release PR. Override the triple with
`make dist TARGET_TRIPLE=x86_64-apple-darwin` (build that target first).

Re-running the `release` workflow on `master` re-uploads the archives for the
current release; `--clobber` makes that idempotent.

## Caveats

- **The binaries are unsigned.** macOS quarantines anything downloaded from a
  browser, so users need `xattr -d com.apple.quarantine ./knobctl` (documented in
  the README's install section). Signing and notarizing would need an Apple
  Developer account and a certificate in repo secrets.
- **The Linux builds are glibc-dynamic**, built on the current runner images, so
  they need a reasonably recent glibc. A static musl build would lift that, but
  `hidapi` links `libudev`, which makes fully-static awkward.
- **The ARM Linux runner (`ubuntu-24.04-arm`) is free for public repositories.**
  On a private repo that leg costs runner minutes, or drop it from the matrix.
- **Nothing is published to crates.io.** If you want that, add a `cargo publish`
  step gated on `release_created` and a `CRATES_IO_TOKEN` secret — the package
  metadata in `Cargo.toml` is already complete enough for it.
