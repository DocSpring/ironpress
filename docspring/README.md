# DocSpring's ironpress build

The `docspring` branch is ironpress 1.7.0 plus the changes below. DocSpring uses it until an upstream release
includes them, then goes back to the upstream packages.

| Change | Upstream |
| --- | --- |
| magnus 0.9, so the Ruby binding compiles against Ruby 4.0 | `33c1ba8` on `main`, not released yet |
| Native gems for Ruby 4.0 | gastongouron/ironpress#275 |
| A box taller than the rest of the page keeps fragmenting when a forced break follows it (a long table before `break-before: page` lost its later rows) | gastongouron/ironpress#276 (its second commit) |
| A table row split by a forced break inside a cell ends its first fragment at the page end, instead of running past it by the table's top border, padding and spacing (with the fix above, that overflow pushed the row onto an extra page) | gastongouron/ironpress#276 (its first commit) |

The Ruby gems add a fourth part to the version (`1.7.0.2` is 1.7.0 plus our second build), so they never collide with
an upstream release. `scripts/check-release-versions.sh` ignores that part. Only the native gems are published: the
source gem would build against the unpatched 1.7.0 crate from crates.io.

## Builds

- `docspring-1.7.0.2`: everything in the table.
- `docspring-1.7.0.1`: the first three rows. Don't use it: a table with a forced break inside a cell can move onto an
  extra page.

## Releasing

1. Push this branch and run both workflows on it:
   `gh workflow run ci.yml --repo DocSpring/ironpress --ref docspring -f publish_crate=false` and
   `gh workflow run ruby.yml --repo DocSpring/ironpress --ref docspring -f publish=false`.
2. When both are green, push a signed tag `docspring-1.7.0.<n>` and create a GitHub release for it with the native
   gems from the Ruby Release run's artifacts, `npm pack` of the CI run's `wasm-package` artifact and a `SHA256SUMS`
   of them all. Releases on this fork are immutable (turned on after `docspring-1.7.0.2`, which isn't), so pass every
   file to the one `gh release create` call: it uploads them to a draft and then publishes it, and nothing can be
   added or replaced afterwards. The notes can still be edited.
3. `gh workflow run publish.yml --repo DocSpring/gems` adds the gems to https://docspring.github.io/gems/.
