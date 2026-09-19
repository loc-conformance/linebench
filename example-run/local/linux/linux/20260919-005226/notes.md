# Benchmark session notes 20260919-005226

corpus:   linux @ 0ff41df1c, pinned and verified
          /home/petros/Documents/dev/bench/linux
          ext4 /dev/nvme0n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4
machine:  cpu governor on 16 cpus: powersave -> performance
          control drift start to end: 1.0008
          background before the run: 0.1% busy
MS Defender: realtime not applicable, process exclusions not applicable
mezura@dev-v3.2.0:v3.2.0 (unreleased) LOCAL BUILD dev-v3.2.0, sha256 ef06a2142
scc:      scc version 4.1.0 fetched, scc_Linux_x86_64.tar.gz
parity:   within 1.0% of the corpus

- [ ] machine quiet during the run

observations:
-

since: no earlier run comparable with this one shares an instance with it
  not compared with 2 earlier runs, the newest 20260912-134847: those runs were with the corpus on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4
