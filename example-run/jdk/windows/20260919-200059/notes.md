# Benchmark session notes 20260919-200059

corpus:       jdk @ b96680ca9, pinned and verified
              D:/dev/bench-corpora/jdk
              NTFS, Lexar SSD NQ790 2TB, SSD, NVMe
machine:      power scheme: Balanced -> high performance
              control drift start to end: 1.0056
              background before the run: 0.4% busy
MS Defender:  realtime True, every process excluded
mezura:       v3.2.0 (2026-09-19) fetched, mezura-v3.2.0-windows-x86_64.zip
scc:          scc version 4.1.0 fetched, scc_Windows_x86_64.zip
tokei:        tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:       within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260913-052324 (same machine, same corpus commit)
  mezura  t1   162 ms -> 140 ms   -13.6% ± 1.5%   version v3.1.1 (2026-09-11) -> v3.2.0 (2026-09-19)
  scc     t1   290 ms -> 280 ms   -3.7% ± 3.3%
  tokei   t1   452 ms -> 448 ms   -1.0% ± 3.3%   within the noise
  control mezura   timed under another build in 20260913-052324, so the machine's own shift is not known
  differs background 1.3% busy -> 0.4% busy
          linebench  0.1.0 -> 0.2.0
          drift      1.3% -> 0.6%
