# Benchmark session notes 20260913-052324

corpus:   jdk @ b96680ca9, pinned and verified
          D:/dev/bench-corpora/jdk
          NTFS, Lexar SSD NQ790 2TB, SSD, NVMe
machine:  power scheme: Balanced -> high performance
          control drift start to end: 1.0134
          background before the run: 1.3% busy
MS Defender: realtime True, every process excluded
mezura:   v3.1.1 (2026-09-11) fetched, mezura-v3.1.1-windows-x86_64.zip
scc:      scc version 4.1.0 fetched, scc_Windows_x86_64.zip
tokei:    tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:   within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260911-134307 (same machine, same corpus commit, same builds)
  mezura         t1   199 ms -> 162 ms   -18.7% ± 5.7%   within the noise
  scc            t1   345 ms -> 290 ms   -15.8% ± 5.1%   within the noise
  tokei          t1   499 ms -> 452 ms   -9.4% ± 2.9%
  control        mezura                     -20.1% ± 3.2%   the machine itself
  the machine itself moved by 20.1%, read the changes against that
  differs        background     5.5% busy -> 1.3% busy
                 linebench      unknown -> 0.1.0
                 drift          7.4% -> 1.3%
