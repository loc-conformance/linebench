# Benchmark session notes 20260919-200315

corpus:       linux @ 0ff41df1c, pinned and verified
              D:/dev/bench-corpora/linux
              NTFS, Lexar SSD NQ790 2TB, SSD, NVMe
machine:      power scheme: Balanced -> high performance
              control drift start to end: 1.0057
              background before the run: 0.1% busy
MS Defender:  realtime True, every process excluded
mezura:       v3.2.0 (2026-09-19) fetched, mezura-v3.2.0-windows-x86_64.zip
scc:          scc version 4.1.0 fetched, scc_Windows_x86_64.zip
tokei:        tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:       within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260919-183617 (same machine, same corpus commit, same builds)
  mezura  t1   198 ms -> 197 ms   -0.6% ± 2.0%   within the noise
  scc     t1   515 ms -> 526 ms   +2.2% ± 2.0%   within the noise
  tokei   t1   614 ms -> 617 ms   +0.5% ± 0.9%   within the noise
  control mezura   +1.2% ± 2.1%   the machine itself
  differs background 0.3% busy -> 0.1% busy
          drift      2.2% -> 0.6%
