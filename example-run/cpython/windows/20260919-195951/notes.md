# Benchmark session notes 20260919-195951

corpus:       cpython @ 34439b8a2, pinned and verified
              D:/dev/bench-corpora/cpython
              NTFS, Lexar SSD NQ790 2TB, SSD, NVMe
machine:      power scheme: Balanced -> high performance
              control drift start to end: 1.0219
              background before the run: 4.6% busy
MS Defender:  realtime True, every process excluded
mezura:       v3.2.0 (2026-09-19) fetched, mezura-v3.2.0-windows-x86_64.zip
scc:          scc version 4.1.0 fetched, scc_Windows_x86_64.zip
tokei:        tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:       within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260913-052216 (same machine, same corpus commit)
  mezura  t1   27 ms -> 23 ms   -13.1% ± 0.3%   version v3.1.1 (2026-09-11) -> v3.2.0 (2026-09-19)
  scc     t1   40 ms -> 41 ms   +2.1% ± 0.5%
  tokei   t1   65 ms -> 67 ms   +2.5% ± 1.0%
  control mezura   timed under another build in 20260913-052216, so the machine's own shift is not known
  differs background 0.2% busy -> 4.6% busy
          linebench  0.1.0 -> 0.2.0
          drift      7.5% -> 2.2%
