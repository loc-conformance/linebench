# Benchmark session notes 20260913-052216

corpus:   cpython @ 34439b8a2, pinned and verified
          D:/dev/bench-corpora/cpython
          NTFS, Lexar SSD NQ790 2TB, SSD, NVMe
machine:  power scheme: Balanced -> high performance
          control drift start to end: 1.0753
          background before the run: 0.2% busy
MS Defender: realtime True, every process excluded
mezura:   v3.1.1 (2026-09-11) fetched, mezura-v3.1.1-windows-x86_64.zip
scc:      scc version 4.1.0 fetched, scc_Windows_x86_64.zip
tokei:    tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:   within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260912-055138 (same machine, same corpus commit, same builds)
  mezura         t1   29 ms -> 27 ms   -6.6% ± 1.0%   within the noise
  scc            t1   55 ms -> 40 ms   -27.0% ± 4.3%
  tokei          t1   68 ms -> 65 ms   -3.2% ± 0.8%   within the noise
  control        mezura                     -3.7% ± 3.7%   the machine itself
  the machine itself moved by 3.7%, read the changes against that
  differs        power          Balanced -> High performance
                 prepared       none -> power scheme: Balanced -> high performance
                 background     1.5% busy -> 0.2% busy
                 antivirus      was  whether the counters are excluded from scanning could not be read (needs admin)
                                now  every counter equally excluded from real-time scanning
                 linebench      unknown -> 0.1.0
                 drift          1.6% -> 7.5%
