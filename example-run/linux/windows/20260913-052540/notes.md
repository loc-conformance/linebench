# Benchmark session notes 20260913-052540

corpus:   linux @ 0ff41df1c, pinned and verified
          D:/dev/bench-corpora/linux
          NTFS, Lexar SSD NQ790 2TB, SSD, NVMe
machine:  power scheme: Balanced -> high performance
          control drift start to end: 1.0278
          background before the run: 0.4% busy
MS Defender: realtime True, every process excluded
mezura:   v3.1.1 (2026-09-11) fetched, mezura-v3.1.1-windows-x86_64.zip
scc:      scc version 4.1.0 fetched, scc_Windows_x86_64.zip
tokei:    tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:   within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260911-014803 (same machine, same corpus commit)
  mezura         t1   the languages, the same-work flags or the instance's own arguments changed, so the times do not compare   version v3.1.0 (2026-09-11) -> v3.1.1 (2026-09-11)
  scc            t1   564 ms -> 539 ms   -4.4% ± 4.6%   within the noise
  tokei          t1   632 ms   never measured before on this machine
  control        mezura                     timed under another build in 20260911-014803, so the machine's own shift is not known
  differs        background     1.2% busy -> 0.4% busy
                 linebench      unknown -> 0.1.0
                 drift          2.1% -> 2.8%
