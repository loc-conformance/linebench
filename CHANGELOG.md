# Changelog

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
