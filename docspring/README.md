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
| `pdf_forms` (off by default, `pdfForms` in JS, `--pdf-forms` on the CLI) turns `<input>` (text-like types, checkbox, radio), `<textarea>` and `<select>` into fillable AcroForm fields over the control's padding box, as WeasyPrint's option does. Values are drawn in Helvetica (WinAnsi) by the fields' appearance streams, and not by the page. Radio buttons with one `name` are one field; other repeated names get `_2`, `_3`. Not yet: a control that is itself a flex item (its text block becomes a flex cell, which drops it, as it drops the value without the option), and a widget doesn't turn with a transformed ancestor | Not sent yet (`docspring-1.7.0.4`) |
| A flex row places an item by its margins once: an item that keeps its own box (a background, or since `docspring-1.7.0.3` a `margin-left`, width or height) was offset by its `margin-left` and `margin-top` again inside its cell, so every item after a gap moved right by the gap, and a stretched item with vertical margins overflowed the row | Not sent yet (`docspring-1.7.0.4`) |
| With `pdf_forms`, `<input type="signature">` (not an HTML input type) becomes an empty signature field (`/FT /Sig`, and `/SigFlags 1` on the form) over its box, which someone signs in a PDF reader. Without `pdf_forms` it is an empty box | Not sent yet (`docspring-1.7.0.5`) |
| The WebAssembly build has an 8 MiB stack (`.cargo/config.toml`), so a document nested as deep as ironpress allows (500 levels) renders. With Rust's 1 MiB default, 23 nested `<div>`s trapped with "memory access out of bounds", and every later call trapped too | Not sent yet (`docspring-1.7.0.6`) |
| The npm package includes `THIRD_PARTY_NOTICES`: the licenses of the crates and the Liberation fonts in the WebAssembly build, each crate with a link to its source (`docspring/notices/build.sh`, with cargo-about; CI checks it's up to date) | Not sent yet (`docspring-1.7.0.6`) |
| The nesting depth limit (500) counts how deeply elements nest: void elements (`<br>`, `<input>`), self-closing tags, comments and the doctype no longer count as levels, a `/` or `>` inside a quoted attribute no longer ends or closes a tag, and the text of `<script>`, `<style>` and other raw text elements is skipped. A flat document with about 500 `<br>`s or comments was refused as nested too deep | Not sent yet (`docspring-1.7.0.7`) |
| `@font-face`, `@page`, margin-box and `@footnote` declarations split only on a `;` outside strings and functions: `src: url("data:font/ttf;base64,...")` was cut at `data:font/ttf`, so no font from a `data:` URL loaded, and `content: "Draft; do not copy"` lost its text | Not sent yet (`docspring-1.7.0.8`) |
| An element nested more than 31 deep fails the conversion with a layout error instead of being left out with everything in it (the PDF came out without that content). `max_nesting_depth` (`maxNestingDepth` in JS, `--max-nesting-depth` on the CLI) raises the limit for a larger stack: each level takes about 60 KB for a flex container in a release build | Not sent yet (`docspring-1.7.0.8`) |
| A row flex item's bottom margin counts in `align-items: flex-end`, `center` and `stretch`: `flex-end` put the item's border box at the line's end and then moved it down by its top margin, so a `<p>` in a flex row sat below where browsers put it, and `stretch` ran into the bottom margin | Not sent yet (`docspring-1.7.0.8`) |
| An element's max-content width (a flex item's, a `width: max-content` box's) collapses whitespace across text nodes and inline elements, as layout does: it trimmed each text node's spaces, so `Page <span>1</span>` measured a space short and a flex item holding it wrapped the span onto a second line | Not sent yet (`docspring-1.7.0.8`) |
| An inline-block nested in three or more others draws its text: the third level was laid out without the layout environment, which collected only text runs, so a fourth inside it was left out with everything it held | Not sent yet (`docspring-1.7.0.8`) |
| Each auto-height grid item is measured once per layout: row sizing laid it out to measure it and the cell pass again, so the work doubled with every level of nested grids (twenty took about a minute) | Not sent yet (`docspring-1.7.0.8`) |
| An element's min- and max-content widths include its `::before` and `::after` text, so a flex item holding `Page <span class="page"></span>` with the number in `::after` keeps the number on its line | Not sent yet (`docspring-1.7.0.8`) |
| A flex item's blocks keep their own `text-align`: they were merged into one cell whose lines were all left-aligned | Not sent yet (`docspring-1.7.0.8`) |
| Elements inside inline content are drawn: an `<img>` in a link, the blocks in an inline-block (on lines of their own), an inline-block or inline-flex in a `<span>`, and inline-blocks in an inline-block share its line. The run collector dropped every element there that wasn't text, with its contents | Not sent yet (`docspring-1.7.0.8`) |
| A `::before` or `::after` inline-block sits on the line's baseline by its last line, as an element's does, and is as wide as its measured text: its bottom edge sat on the baseline and its width came from estimated advances | Not sent yet (`docspring-1.7.0.8`) |
| A flex item with `flex: 1` (or any zero basis) holding its own text is as tall as that text at its resolved width: it was wrapped a word to a line at the basis, and the flex line kept that height | Not sent yet (`docspring-1.7.0.8`) |
| A flex or grid container that is itself a flex item lays out its inline children as its items, instead of as one line of text | Not sent yet (`docspring-1.7.0.8`) |
| An auto-width grid fills its containing block with its margin box (CSS 2.1 §10.3.3): without horizontal margins its borders were added outside that width, so it overflowed by their width | Not sent yet (`docspring-1.7.0.8`) |
| Flex shrinking scales each item by its inner base size (css-flexbox §9.7), not its outer size, so items with padding or borders shrink by the amounts browsers use | Not sent yet (`docspring-1.7.0.8`) |
| A flex item with a `width` is laid out at its flexed size: its contents were laid out at the authored width after the item grew or shrank | Not sent yet (`docspring-1.7.0.8`) |
| Pair kerning continues across inline boundaries that change only paint (a `<span>` with a colour, `::before` content), as Chromium shapes text: `“<span>Q</span>` was wider than `“Q`. A margin, border or padding between the glyphs still separates them. Right-to-left text doesn't kern across a boundary yet | Not sent yet (`docspring-1.7.0.8`) |
| A document wider than the page shrinks to fit as Chromium prints it: lines of text that overflow their box and flex items wider than their row count towards its width (a box that clips its overflow keeps it inside), and the document is laid out again at the wider page, up to one and a half times as wide, before it is scaled down, so text wraps where Chromium wraps it | Not sent yet (`docspring-1.7.0.8`) |
| The first child's top margin collapses through `<body>` only when the body has no top padding or border: with body padding, the margin was dropped and the child sat at the top of the padding | Not sent yet (`docspring-1.7.0.8`) |
| An inline `<svg>`, or an `<img>` with an SVG source, inside a `<span>` is drawn in its line, and the span's background is painted behind it. A line of inline boxes in a block with a definite `height` keeps that height | Not sent yet (`docspring-1.7.0.8`) |
| An absolutely positioned box whose insets are `auto` on an axis sits at its static position on that axis (CSS 2.1 §10.3.7, §10.6.4): below the box before it, at its parent's content edge, or, in a line of text, where it falls among the words. One that was block-level starts below its line. An absolute box inside a `<span>` was left out with everything in it | Not sent yet (`docspring-1.7.0.8`) |
| An absolutely positioned box draws its text in its content box, inside its left border and padding, with its `text-align`, `text-indent`, inline backgrounds, background gradients and overflow clip, as a box in flow does: its text started at its left border edge, so a right-anchored box with padding was pulled left by its padding | Not sent yet (`docspring-1.7.0.8`) |
| An absolute box with no positioned ancestor takes its `top`, `right` and `bottom` from the page area, the initial containing block, instead of from its parent, and a `position: fixed` box is placed in the page area, not inside `<body>`'s margin and padding. A positioned table cell or caption is the containing block of the absolute boxes in it, and so is a positioned box in a multi-column container | Not sent yet (`docspring-1.7.0.8`) |

The Ruby gems add a fourth part to the version (`1.7.0.2` is 1.7.0 plus our second build), so they never collide with
an upstream release. `scripts/check-release-versions.sh` ignores that part. Only the native gems are published: the
source gem would build against the unpatched 1.7.0 crate from crates.io.

## Builds

- `docspring-1.7.0.3` to `docspring-1.7.0.8`: the rows up to and including that build's.
- `docspring-1.7.0.2`: the first four rows.
- `docspring-1.7.0.1`: the first three rows. Don't use it: a table with a forced break inside a cell can move onto an
  extra page.

## Parity suite

`tests/parity/` compares ironpress's pages with Chromium's, fixture by fixture (`tests/parity/README.md`). It needs
Ubuntu 24.04 on amd64 with pinned fonts and Poppler, so we run it in Docker on an amd64 Linux host:
`docspring/parity/run.sh scripts/parity.sh` builds the image (`docspring/parity/Dockerfile`, with Chrome for Testing
for new oracles) and runs the command in it with the checkout at `/src`. A fix that changes layout gets a fixture
with a Chromium oracle (`docspring/parity/run.sh scripts/parity-gen-refs.sh <category>`), and a run that improves a
fixture updates the baseline (`PARITY_UPDATE_BASELINE=1`).

## Releasing

After changing dependencies or `Cargo.lock`, run `docspring/notices/build.sh` (it needs
`cargo install cargo-about --locked --version 0.8.4`) and commit `THIRD_PARTY_NOTICES`: the `wasm` job fails while it's
out of date.

1. Push this branch and run both workflows on it:
   `gh workflow run ci.yml --repo DocSpring/ironpress --ref docspring -f publish_crate=false` and
   `gh workflow run ruby.yml --repo DocSpring/ironpress --ref docspring -f publish=false`.
2. When both are green, push a signed tag `docspring-1.7.0.<n>` and create a GitHub release for it with the native
   gems from the Ruby Release run's artifacts, `npm pack` of the CI run's `wasm-package` artifact and a `SHA256SUMS`
   of them all. Releases on this fork are immutable (turned on after `docspring-1.7.0.2`, which isn't), so pass every
   file to the one `gh release create` call: it uploads them to a draft and then publishes it, and nothing can be
   added or replaced afterwards. The notes can still be edited.
3. `gh workflow run publish.yml --repo DocSpring/gems` adds the gems to https://docspring.github.io/gems/.
