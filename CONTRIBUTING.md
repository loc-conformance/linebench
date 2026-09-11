# Contributing

## Adding a counter

A counter is one file, `counters/<name>.toml`. Its keys mirror the linejudge adapter where the
idea is the same (`name`, `repository`, `version-flag`, `[acquisition]`, the `[read]` paths), so
a block copies between the two files unchanged. The README's "Counters and corpora" section
shows a whole one and says what every key does. Before opening a pull request:

```
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
linebench fetch --counters all --corpus cpython
linebench check cpython
```

The tests run git and hyperfine, so both have to be on PATH before `cargo test`.

`fetch` brings the counter the way its definition says, from an ordinary terminal. `check` is
the acceptance test: it runs the fetched counter once per table, reads the counts back, and
compares them with the file count the corpus declares. CI runs both over the cpython corpus on
Linux, Windows and macOS. Run `check` on the `linux` corpus too before publishing a definition:
the kernel is where cloc's case handling and tokei's nested languages were found.

## What has already bitten

- **A counter that is a script on Windows is not covered.** cloc ships `cloc-2.10.exe` there and
  `cloc-2.10.pl` elsewhere, so it is its own process everywhere. A counter that is only a script
  needs a per-system interpreter table in `[run]`, and the process name the antivirus sees would
  come from the interpreter. It is written the day such a counter exists.
- **`[acquisition.file]` is keyed by system alone**: `windows`, `linux`, `macos`, `other`. A
  release that publishes named files per architecture cannot be declared yet; the
  `github-release-asset` channel already picks by system and architecture words, and the file
  channel would take the same keys.
- **The fetched file is stored under the counter's own name** plus the release file's extension,
  `cloc.exe` and `cloc.pl`, whatever the release calls it. Windows Defender matches process
  exclusions by image name, so the name has to survive a version bump. The release file's name
  still goes into `linebench-fetched.toml` as the source.
- **A counter that matches an extension by its exact case says `extension-case = "exact"`.**
  Measured on the kernel: cloc on Linux given `--include-ext=s` skipped all 1,350 `.S` files,
  and on Windows given `S` it matched nothing, so the harness spells both cases, `s,S`.
- **A counter whose JSON the path locator cannot express needs a compiled reader**, declared as
  `output = "<name>"` with no `[read]` block. tokei prints a map of languages with no file count
  and a `Total` that adds every language, its children and the blobs nested inside them, so
  `tokei-json` reconciles the three before it trusts the total. The locator walks arrays with
  `[]` and cannot say "every value of an object", which is why cloc's per-language rows go unread
  and the paths point at its `SUM` row.
- **`[read]` has to name a third bucket beside `code` and `comments`**, `blanks` for most, and
  the buckets have to add up to `lines`. A document that does not add up is refused with the
  arithmetic, which is what catches a wrong path.
- **What `--version` prints has to contain the declared version as a whole token.** `4.0.0` is
  not found inside `14.0.0`. A counter that prints its version in another form needs
  `version-flag` pointed at whatever does print it.
- **`crates-io` means cargo on the measuring machine.** It is the channel of last resort, for a
  counter that publishes no binaries at all; the refusal names the `cargo install` line and
  `--given` as the way around it.
- **`same-work` is the counter's own claim, and the equal work check is what tests it.** A
  definition that turns off less than it should, or selects fewer files, comes out over or under
  the corpus and the run is marked. Look at the four shipped definitions for what "the same
  work" means: the corpus's languages, no ignore file read by anyone, and everything beyond
  counting off.
- **Counters disagree about directories whose name starts with a dot.** Measured on a tree of
  five files: cloc and scc walk into `.github` and stay out of `.git`, while tokei and mezura
  stay out of both. The reference counts what `git ls-tree` lists, `.github` included, so
  same-work carries `--search-in-dotted` for mezura and `--hidden` for tokei. On cpython that is
  two files each; on the kernel `--hidden` sends tokei through a 276 MB `.git` and finds nothing
  in it.
- **A language flag is not an extension flag.** `-i` of scc and `--include-ext` of cloc take
  extensions, while `--languages` of mezura and `-t` of tokei take languages, so `js` brings
  `.mjs` along and `py` brings `.pyi` and `.pyw`. A definition that spells languages by name
  selects a wider set than the corpus names, which is a difference the counts show and the
  tolerance absorbs.

## Adding a corpus

A corpus is `corpora/<name>.toml`: `name`, `remote`, `commit`, `files`, `extensions`,
`tolerance`. The commit is the full 40-character hash in lower case, as `git rev-parse` prints
it; an abbreviated one cannot be fetched with `--depth 1` and is refused. The extensions are
spelled once, in whatever case, and each counter spells them its own way. A counter that spells
languages by name, tokei alone today, needs every extension in its `[language-names]` table,
and the shipped one covers the extensions the shipped corpora name. An extension missing from
it stops the run before anything is measured, naming the extension and the file to add it to.
`files` is the number of files carrying them at that commit, required beside a commit: run
`check` on the checkout, read what the counters answer, and write the number you trust. The
tolerance belongs to the corpus because how many odd files a tree holds is a property of the
tree. `[skip]` names, per system, the counters left out of the default set over this corpus:
the kernel names cloc under windows, linux and macos alike, since a plain run holding it takes
hours.

## Publishing a run

A run worth publishing is measured elevated, so the power scheme or governor is set for it, on a
machine with nothing else running, and its trust checks say so: a control drift under a few
percent, both command orders within a few percent, background well under one core. A run
holding a build given by hand lands under `results/local/` and never on the page's release
tables. `results/README.md` is generated; do not edit it by hand.

Publish it from your own repository: copy `results/README.md` and the run directories it names
into a folder there, put the insight sessions beside them if you ran any, and link to that folder
from your README. Nothing under `results/` is committed here.
