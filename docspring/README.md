# DocSpring's ironpress build

The `docspring` branch is ironpress 1.7.0 plus the changes below. DocSpring uses it until an upstream release
includes them, then goes back to the upstream packages.

| Change | Upstream |
| --- | --- |
| magnus 0.9, so the Ruby binding compiles against Ruby 4.0 | `33c1ba8` on `main`, not released yet |
| Native gems for Ruby 4.0 | gastongouron/ironpress#275 |
| A box taller than the rest of the page keeps fragmenting when a forced break follows it (a long table before `break-before: page` lost its later rows) | gastongouron/ironpress#276 (its second commit) |
| A table row split by a forced break inside a cell ends its first fragment at the page end, instead of running past it by the table's top border, padding and spacing (with the fix above, that overflow pushed the row onto an extra page) | gastongouron/ironpress#276 (its first commit) |
| Inline content (text, an `<img>`, a `<span>`) between block children keeps its place, in an anonymous block, instead of being dropped (in a grid item) or moved above the blocks | Not sent yet (`docspring-1.7.0.3`) |
| Block children of list items and table cells are laid out in source order, with their indent | Not sent yet (`docspring-1.7.0.3`) |
| A block whose children are all blocks keeps its own width, margins, height, transform and overflow | Not sent yet (`docspring-1.7.0.3`) |
| A container's flow height collapses its children's margins, so content after a box with a background isn't placed too low | Not sent yet (`docspring-1.7.0.3`) |
| `counter(page)` and `counter(pages)` in a running element show each page's numbers, not 0 | Not sent yet (`docspring-1.7.0.3`) |
| Page numbers in running elements get room for their widest value | Not sent yet (`docspring-1.7.0.3`) |
| Running elements in corner and side margin boxes are placed in those boxes | Not sent yet (`docspring-1.7.0.3`) |
| An `<ol>` that is a grid item numbers its items from `start`, not 0 | Not sent yet (`docspring-1.7.0.3`) |
| Blocks inside a flex item keep their geometry and margins | Not sent yet (`docspring-1.7.0.3`) |
| Child selectors (`li > b`) match a list item's inline children | Not sent yet (`docspring-1.7.0.3`) |
| Nested unordered lists get circle, then square markers | Not sent yet (`docspring-1.7.0.3`) |
| A list keeps its margins, padding, background, border and width | Not sent yet (`docspring-1.7.0.3`) |
| List items have no UA bottom margin, as in Chrome | Not sent yet (`docspring-1.7.0.3`) |
| A list item's box encloses all its block children | Not sent yet (`docspring-1.7.0.3`) |
| Auto-layout table cells are sized to their blocks, and a cell's paragraph margins count once | Not sent yet (`docspring-1.7.0.3`) |
| Inline `<progress>` and `<meter>` gauges are laid out in their line | Not sent yet (`docspring-1.7.0.3`) |
| Inline-blocks in a grid item's lines are painted | Not sent yet (`docspring-1.7.0.3`) |
| Inline-blocks nested in an inline-block are painted | Not sent yet (`docspring-1.7.0.3`) |
| `::before` and `::after` content on an inline-block is laid out | Not sent yet (`docspring-1.7.0.3`) |
| `blockquote` has Chrome's UA margins and isn't italic | Not sent yet (`docspring-1.7.0.3`) |
| A 16-bit PNG with alpha is drawn at its size and in its colours (its samples were read as twice as many 8-bit pixels), a palette PNG is drawn as a CSS background (it drew nothing), and a palette's or `tRNS` chunk's transparency is kept | Not sent yet (`docspring-1.7.0.4`) |

The Ruby gems add a fourth part to the version (`1.7.0.2` is 1.7.0 plus our second build), so they never collide with
an upstream release. `scripts/check-release-versions.sh` ignores that part. Only the native gems are published: the
source gem would build against the unpatched 1.7.0 crate from crates.io.

## Builds

- `docspring-1.7.0.4`: everything in the table.
- `docspring-1.7.0.3`: everything but the last row.
- `docspring-1.7.0.2`: the first four rows.
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
