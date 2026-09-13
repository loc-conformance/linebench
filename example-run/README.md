# Benchmark results

Written by `linebench report` after every run, and rewritten whole each time. One section per machine, and under it the newest run over each corpus, biggest corpus first. Older runs are listed in the "Every run" table further down. A run holding a build or arguments of your own is kept apart, under its own headings. What every term means and how this was measured: the last two sections.

## Windows, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 62 GB usable RAM, Microsoft Windows 11 Pro

### linux corpus, 20260913-052540

measured 2026-09-13 05:25 UTC by linebench 0.1.0  
corpus at `0ff41df1c` on NTFS, Lexar SSD NQ790 2TB, SSD, NVMe  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 254 ms ± 22 | 1.00x | 1.92 s | 1.45 s | 13.24 | 141.6M | 10.7M | 63,767 | 36,017,775 |
| scc | 539 ms ± 61 | 2.12x ± 0.30 | 3.53 s | 3.85 s | 13.69 | 66.8M | 4.9M | 63,767 | 36,017,775 |
| tokei | 632 ms ± 22 | 2.48x ± 0.23 | 6.02 s | 2.66 s | 13.73 | 57.0M | 4.2M | 63,822 | 36,026,522 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 300 ms ± 15 | 1.00x | 2.15 s | 1.81 s | 13.20 | 119.3M | 9.0M | 66,568 | 35,815,794 |
| tokei | 734 ms ± 32 | 2.45x ± 0.16 | 6.84 s | 3.26 s | 13.76 | 54.5M | 4.0M | 83,891 | 39,991,863 |
| scc | 737 ms ± 35 | 2.46x ± 0.17 | 5.80 s | 4.77 s | 14.33 | 54.2M | 3.8M | 83,832 | 39,992,940 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 2.8%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 6.8%.
- **Power**: set for the run and restored after: power scheme: Balanced -> high performance.
- **Quiet machine**: everything other than the benchmark was using 0.4% of the cpu when the run started, about 0.1 of 16 cores.
- **Antivirus**: real-time protection True, every counter equally excluded from real-time scanning.
- **Equal work**: every file count sat within 1.0% of the 63,779 files the corpus declares, and the line counts within 1.0% of each other.

### jdk corpus, 20260913-052324

measured 2026-09-13 05:23 UTC by linebench 0.1.0  
corpus at `b96680ca9` on NTFS, Lexar SSD NQ790 2TB, SSD, NVMe  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 162 ms ± 12 | 1.00x | 1.07 s | 1.23 s | 14.22 | 81.7M | 5.7M | 61,425 | 13,226,930 |
| scc | 290 ms ± 18 | 1.79x ± 0.17 | 1.78 s | 2.01 s | 13.06 | 45.5M | 3.5M | 61,420 | 13,226,395 |
| tokei | 452 ms ± 24 | 2.79x ± 0.26 | 4.04 s | 2.09 s | 13.54 | 29.2M | 2.2M | 61,421 | 13,226,397 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 182 ms ± 11 | 1.00x | 1.26 s | 1.36 s | 14.43 | 75.6M | 5.2M | 61,950 | 13,740,143 |
| scc | 397 ms ± 19 | 2.18x ± 0.17 | 2.89 s | 2.55 s | 13.70 | 39.8M | 2.9M | 66,301 | 15,818,623 |
| tokei | 540 ms ± 19 | 2.97x ± 0.21 | 5.22 s | 2.33 s | 13.99 | 29.1M | 2.1M | 65,042 | 15,688,680 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 1.3%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 6.3%.
- **Power**: set for the run and restored after: power scheme: Balanced -> high performance.
- **Quiet machine**: everything other than the benchmark was using 1.3% of the cpu when the run started, about 0.2 of 16 cores.
- **Antivirus**: real-time protection True, every counter equally excluded from real-time scanning.
- **Equal work**: every file count sat within 1.0% of the 61,420 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: control-start: Statistical outliers were detected.
- **hyperfine warning**: t1-fwd: Statistical outliers were detected.
- **hyperfine warning**: t1-rev: Statistical outliers were detected.
- **hyperfine warning**: t2-fwd: Statistical outliers were detected.
- **hyperfine warning**: t2-rev: Statistical outliers were detected.
- **hyperfine warning**: control-end: Statistical outliers were detected.

### cpython corpus, 20260913-052216

measured 2026-09-13 05:22 UTC by linebench 0.1.0  
corpus at `34439b8a2` on NTFS, Lexar SSD NQ790 2TB, SSD, NVMe  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 27 ms ± 0 | 1.00x | 0.10 s | 0.13 s | 8.85 | 82.7M | 9.3M | 3,559 | 2,230,164 |
| scc | 40 ms ± 0 | 1.48x ± 0.02 | 0.19 s | 0.18 s | 9.29 | 56.0M | 6.0M | 3,556 | 2,229,545 |
| tokei | 65 ms ± 1 | 2.42x ± 0.05 | 0.47 s | 0.33 s | 12.13 | 34.1M | 2.8M | 3,583 | 2,230,905 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 31 ms ± 0 | 1.00x | 0.15 s | 0.13 s | 9.04 | 69.0M | 7.6M | 3,577 | 2,127,111 |
| scc | 58 ms ± 1 | 1.87x ± 0.03 | 0.33 s | 0.29 s | 10.83 | 52.9M | 4.9M | 5,780 | 3,047,697 |
| tokei | 81 ms ± 2 | 2.62x ± 0.07 | 0.61 s | 0.40 s | 12.40 | 37.1M | 3.0M | 5,659 | 2,998,726 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 7.5%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 0.2%.
- **Power**: set for the run and restored after: power scheme: Balanced -> high performance.
- **Quiet machine**: everything other than the benchmark was using 0.2% of the cpu when the run started, about 0.0 of 16 cores.
- **Antivirus**: real-time protection True, every counter equally excluded from real-time scanning.
- **Equal work**: every file count sat within 1.0% of the 3,556 files the corpus declares, and the line counts within 1.0% of each other.

## Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

### linux corpus, 20260912-134847

measured 2026-09-12 13:48 UTC by linebench 0.1.0  
corpus at `0ff41df1c` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 166 ms ± 12 | 1.00x | 1.81 s | 0.38 s | 13.17 | 216.8M | 16.5M | 63,738 | 36,018,801 |
| scc | 210 ms ± 3 | 1.26x ± 0.09 | 2.78 s | 0.37 s | 15.00 | 171.4M | 11.4M | 63,738 | 36,018,801 |
| tokei | 414 ms ± 2 | 2.50x ± 0.18 | 5.59 s | 0.60 s | 14.94 | 86.9M | 5.8M | 63,793 | 36,027,518 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 197 ms ± 12 | 1.00x | 2.27 s | 0.39 s | 13.54 | 181.8M | 13.4M | 66,539 | 35,816,820 |
| scc | 321 ms ± 4 | 1.63x ± 0.10 | 4.46 s | 0.45 s | 15.27 | 124.5M | 8.1M | 83,784 | 39,993,947 |
| tokei | 467 ms ± 3 | 2.37x ± 0.15 | 6.29 s | 0.73 s | 15.03 | 85.6M | 5.7M | 83,843 | 39,992,832 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 0.9%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 2.4%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 63,779 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: t2-fwd: Statistical outliers were detected.

### jdk corpus, 20260912-134711

measured 2026-09-12 13:47 UTC by linebench 0.1.0  
corpus at `b96680ca9` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 82 ms ± 4 | 1.00x | 0.88 s | 0.30 s | 14.48 | 162.2M | 11.2M | 61,425 | 13,226,930 |
| scc | 114 ms ± 9 | 1.40x ± 0.13 | 1.36 s | 0.32 s | 14.70 | 115.6M | 7.9M | 61,420 | 13,226,395 |
| tokei | 304 ms ± 4 | 3.73x ± 0.19 | 4.06 s | 0.43 s | 14.75 | 43.4M | 2.9M | 61,421 | 13,226,397 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 96 ms ± 7 | 1.00x | 1.10 s | 0.30 s | 14.47 | 142.4M | 9.8M | 61,950 | 13,740,143 |
| scc | 184 ms ± 10 | 1.90x ± 0.17 | 2.37 s | 0.36 s | 14.89 | 86.1M | 5.8M | 66,301 | 15,818,623 |
| tokei | 372 ms ± 3 | 3.85x ± 0.28 | 5.14 s | 0.42 s | 14.96 | 42.2M | 2.8M | 65,042 | 15,688,680 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 2.0%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 2.7%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 61,420 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: t1-fwd: The first benchmarking run for this command was significantly slower than the rest (89.7 ms).
- **hyperfine warning**: t1-fwd: Statistical outliers were detected.
- **hyperfine warning**: t1-rev: Statistical outliers were detected.
- **hyperfine warning**: t1-rev: Statistical outliers were detected.
- **hyperfine warning**: t2-fwd: Statistical outliers were detected.
- **hyperfine warning**: t2-rev: The first benchmarking run for this command was significantly slower than the rest (130.4 ms).
- **hyperfine warning**: control-end: Statistical outliers were detected.

### cpython corpus, 20260912-134614

measured 2026-09-12 13:46 UTC by linebench 0.1.0  
corpus at `34439b8a2` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 17 ms ± 1 | 1.00x | 0.15 s | 0.02 s | 10.33 | 134.3M | 13.0M | 3,557 | 2,230,162 |
| scc | 20 ms ± 0 | 1.21x ± 0.11 | 0.22 s | 0.03 s | 12.23 | 111.1M | 9.1M | 3,554 | 2,229,543 |
| tokei | 45 ms ± 1 | 2.74x ± 0.24 | 0.62 s | 0.03 s | 14.24 | 49.1M | 3.4M | 3,581 | 2,230,903 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 22 ms ± 15 | 1.00x | 0.17 s | 0.02 s | 8.68 | 97.3M | 11.2M | 3,573 | 2,124,279 |
| scc | 31 ms ± 0 | 1.43x ± 0.96 | 0.36 s | 0.04 s | 12.84 | 97.5M | 7.6M | 5,777 | 3,047,694 |
| tokei | 55 ms ± 1 | 2.50x ± 1.69 | 0.73 s | 0.04 s | 14.09 | 54.9M | 3.9M | 5,657 | 2,998,673 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 15.4%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 31.0%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 3,556 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: t2-fwd: Statistical outliers were detected.
- **hyperfine warning**: control-end: Statistical outliers were detected.

## Local builds, Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

Runs holding an instance given by hand, a build of your own or a release carrying arguments of its own. They compare one build with another on this machine and say nothing about the released counters.

### linux corpus, 20260906-220447

measured 2026-09-06 22:04 UTC  
corpus at `0ff41df1c` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.0.0 (2026-09-02), mezura@v3.1.0 v3.1.0 (unreleased) (local build v3.1.0)  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura@v3.1.0 | 167 ms ± 10 | 1.00x | 1.88 s | 0.38 s | 13.56 | 215.9M | 15.9M | 63,864 | 36,036,878 |
| mezura | 224 ms ± 12 | 1.34x ± 0.11 | 2.11 s | 0.35 s | 10.95 | 160.9M | 14.7M | 63,864 | 36,036,878 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura@v3.1.0 | 193 ms ± 5 | 1.00x | 2.25 s | 0.41 s | 13.77 | 185.9M | 13.5M | 66,539 | 35,816,820 |
| mezura | 258 ms ± 8 | 1.34x ± 0.05 | 2.46 s | 0.36 s | 10.95 | 139.0M | 12.7M | 66,539 | 35,816,820 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 0.2%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 3.9%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0.1% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 63,765 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: t1-fwd: Statistical outliers were detected.

## Methodology

- hyperfine, with no shell in between. Each section above states its own warmups, timed runs and pause.
- The machine is restarted and otherwise idle, and the run samples the system-wide cpu before it measures anything, which is the quiet machine trust check above. `linebench noise` answers the same question with five runs of the control, before committing to a run.
- Every table is measured twice, in one command order and then in the reverse. The numbers shown average the two, and how far they disagreed is printed in each run's trust checks.
- A corpus definition pins a commit, and a checkout on any other commit refuses to run. A run on an unpinned tree says so beside its corpus line.
- Counts come from each counter's own JSON output, and the file counts are checked against the count the corpus definition declares for its commit, which is the equal work trust check above.
- Same work: one language set for every counter, generated and minified files counted by all, no ignore file read by any of them, and every extra feature turned off. What each one turns off, as its definition declares it:
  - mezura: ignore files off, dotted directories entered, no default and no project settings read, shebang lines unread, keyword counting off, minified, generated and non code files counted, region counting model, content identification left on since scc has no way to turn its own off
  - scc: ignore files off, complexity and cost estimates off, no config file read
  - tokei: ignore files off, hidden files and directories counted, which is everything tokei has beyond the counting
- Out of the box: bare `counter <dir>` with no flags, plus an instance's own arguments where it has them.
- The exact flags: each counter's definition under `counters/` in the linebench repository.

## Terms

- **wall**: how long a run takes on the clock, in milliseconds: the mean of all the timed runs, both command orders together, ± their σ. That σ holds the run-to-run noise plus half the gap between the two orders.
- **vs fastest**: this counter's wall divided by the fastest counter's wall in the same table, ± the σ of that ratio, worked out from the two walls' σ. A ratio whose interval reaches 1.00 is within the noise of the fastest.
- **user cpu**: cpu seconds spent running the counter's own code, summed over every thread. 16 threads busy for one second is 16 s.
- **system cpu**: cpu seconds spent inside the operating system on the counter's behalf, opening and reading files, plus whatever sits on that path (antivirus, filter drivers).
- **parallelism**: user plus system cpu, divided by wall: 4.6 s of cpu inside a 0.35 s run means 13 threads were busy on average.
- **lines/s**: the lines this counter itself counted, divided by its wall time.
- **lines per cpu second**: the lines this counter counted, divided by its user plus system cpu. How cheaply it counts, with the number of cores taken out of the picture.
- **files / lines**: what the counter reported counting. Under "Same work" every counter must nearly agree, and the equal work check says whether they did. Out of the box they differ by design.
- **machine steadiness**: the same binary timed at the start and at the end of the whole run. The percentage is how far apart the two means came out.
- **since the last run**: each instance's same-work wall against its own latest comparable earlier run, ± the σ of that change from the two means' own σ (σ/√n per order, the order gap kept whole). "within the noise" means the change minus the control's own shift is inside the combined σ.

Every number here comes out of a record under this folder. `linebench report --verify` says whether this page is the one those records make, and `linebench verify <run directory>` reads one of them back and holds its numbers against each other.
