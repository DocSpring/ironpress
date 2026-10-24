# DocSpring's ironpress build

The `docspring` branch is ironpress 1.7.0 plus the changes below. DocSpring uses it until an upstream release
includes them, then goes back to the upstream packages.

| Change | Upstream |
| --- | --- |
| magnus 0.9, so the Ruby binding compiles against Ruby 4.0 | `33c1ba8` on `main`, not released yet |
| Native gems for Ruby 4.0 | gastongouron/ironpress#275 |
| A box taller than the rest of the page keeps fragmenting when a forced break follows it (a long table before `break-before: page` lost its later rows) | branch `fix/table-overflow-before-forced-break` here; upstream PR to follow |

The Ruby gems add a fourth part to the version (`1.7.0.1` is 1.7.0 plus our first build), so they never collide with
an upstream release. `scripts/check-release-versions.sh` ignores that part. Only the native gems are published: the
source gem would build against the unpatched 1.7.0 crate from crates.io.

## Releasing

1. Push this branch and run both workflows on it:
   `gh workflow run ci.yml --repo DocSpring/ironpress --ref docspring -f publish_crate=false` and
   `gh workflow run ruby.yml --repo DocSpring/ironpress --ref docspring -f publish=false`.
2. When both are green, push a signed tag `docspring-1.7.0.<n>` and create a GitHub release for it with the native
   gems from the Ruby Release run's artifacts and `npm pack` of the CI run's `wasm-package` artifact.
3. `gh workflow run publish.yml --repo DocSpring/gems` adds the gems to https://docspring.github.io/gems/.
