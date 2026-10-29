# Changelog

## 0.3.0, 2026-10-29

New:

- `ab` holds a baseline build against one or more candidates on the hardware counters: each pair
  under `perf stat` in the order a, b, b, a over corpora and folders, user-space instructions,
  cycles, wall time and `--events`, count checks, `--json`, and `--files [<n>]` for the largest
  files of a target. Linux, with `kernel.perf_event_paranoid` at 2 or below.
- `insights` counts heap allocations on Linux through `valgrind` in a new `allocations` section:
  allocations, allocations per file, total bytes allocated, bytes per allocation, and bytes in use
  at exit.
- An instance can carry an environment of its own, from `--env <instance>=<NAME>=<value>`, `env` in
  its `[given]` entry, or `[run] env` (with `{instance-dir}`) in its counter definition, and
  `--args <text>` applies arguments across every instance in the run.

Other:

- mezura is declared at 3.3.0.

## 0.2.0, 2026-09-19

New:

- `insights` reads the hardware counters on Linux, in a new `pmu` section: instructions, cycles,
  branch instructions and misses, L1 and last level cache loads and misses, and page faults.
  The section wants root, or `kernel.perf_event_paranoid` at 1.
- The pages a run was given are reported beside its peak memory. On Linux the pages that came from
  disk get their own `from disk` count, and linebench warns about a run that has any, since it was
  reading a cold cache.
- The machine record carries the filesystem UUID of the volume the corpus sits on.

Other:

- mezura is declared at 3.2.0.
- Each label column of the printed tables is as wide as the widest name in it.
