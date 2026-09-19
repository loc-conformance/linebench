# Benchmark session notes 20260919-180321

corpus:       cpython @ 34439b8a2, pinned and verified
              /home/petros/Documents/dev/bench/cpython
              ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4
machine:      cpu governor on 16 cpus: powersave -> performance
              control drift start to end: 1.0023
              background before the run: 0.1% busy
MS Defender:  realtime not applicable, process exclusions not applicable
mezura:       v3.2.0 (2026-09-19) fetched, mezura-v3.2.0-linux-x86_64.tar.gz
scc:          scc version 4.1.0 fetched, scc_Linux_x86_64.tar.gz
tokei:        tokei 15.0.0 compiled with serialization support: json built with rustc 1.97.1 (8bab26f4f 2026-07-14)
parity:       within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since 20260912-134614 (same machine, same corpus commit)
  mezura  t1   17 ms -> 10 ms   -37.9% ± 1.3%   version v3.1.1 (2026-09-11) -> v3.2.0 (2026-09-19)
  scc     t1   20 ms -> 20 ms   -0.2% ± 0.5%   within the noise
  tokei   t1   45 ms -> 45 ms   -0.1% ± 0.6%   within the noise
  control mezura   timed under another build in 20260912-134614, so the machine's own shift is not known
  differs background 0% busy -> 0.1% busy
          linebench  0.1.0 -> 0.2.0
          drift      15.4% -> 0.2%
