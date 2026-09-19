# Benchmark results

Written by `linebench report` after every run, and rewritten whole each time. One section per machine, and under it the newest run over each corpus, biggest corpus first. Older runs are listed in the "Every run" table further down. A run holding a build or arguments of your own is kept apart, under its own headings. What every term means and how this was measured: the last two sections.

## Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

### linux corpus, 20260919-180551

measured 2026-09-19 18:05 UTC by linebench 0.2.0  
corpus at `0ff41df1c` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.2.0 (2026-09-19), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 86 ms ± 3 | 1.00x | 0.90 s | 0.25 s | 13.27 | 417.7M | 31.5M | 63,738 | 36,018,801 |
| scc | 208 ms ± 2 | 2.41x ± 0.09 | 2.76 s | 0.35 s | 14.97 | 173.3M | 11.6M | 63,738 | 36,018,801 |
| tokei | 414 ms ± 2 | 4.80x ± 0.17 | 5.59 s | 0.60 s | 14.97 | 87.1M | 5.8M | 63,793 | 36,027,518 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 104 ms ± 4 | 1.00x | 1.13 s | 0.28 s | 13.52 | 343.2M | 25.4M | 66,539 | 35,816,820 |
| scc | 321 ms ± 3 | 3.07x ± 0.12 | 4.44 s | 0.45 s | 15.27 | 124.8M | 8.2M | 83,784 | 39,993,947 |
| tokei | 465 ms ± 3 | 4.46x ± 0.17 | 6.28 s | 0.71 s | 15.03 | 86.0M | 5.7M | 83,843 | 39,992,832 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 2.4%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 1.4%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 63,779 files the corpus declares, and the line counts within 1.0% of each other.

```
since 20260912-134847 (same machine, same corpus commit)
  mezura  t1   166 ms -> 86 ms   -48.1% ± 1.0%   version v3.1.1 (2026-09-11) -> v3.2.0 (2026-09-19)
  scc     t1   210 ms -> 208 ms   -1.1% ± 0.4%
  tokei   t1   414 ms -> 414 ms   -0.2% ± 0.2%
  control mezura   timed under another build in 20260912-134847, so the machine's own shift is not known
  differs linebench 0.1.0 -> 0.2.0
          drift     0.9% -> 2.4%
```

### jdk corpus, 20260919-180418

measured 2026-09-19 18:04 UTC by linebench 0.2.0  
corpus at `b96680ca9` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.2.0 (2026-09-19), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 52 ms ± 1 | 1.00x | 0.47 s | 0.26 s | 14.02 | 256.2M | 18.3M | 61,425 | 13,226,930 |
| scc | 115 ms ± 10 | 2.22x ± 0.20 | 1.37 s | 0.32 s | 14.72 | 115.3M | 7.8M | 61,420 | 13,226,395 |
| tokei | 304 ms ± 3 | 5.89x ± 0.12 | 4.07 s | 0.43 s | 14.80 | 43.5M | 2.9M | 61,421 | 13,226,397 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 62 ms ± 1 | 1.00x | 0.59 s | 0.26 s | 13.83 | 222.9M | 16.1M | 61,950 | 13,740,143 |
| scc | 184 ms ± 10 | 2.99x ± 0.16 | 2.36 s | 0.36 s | 14.79 | 85.8M | 5.8M | 66,301 | 15,818,623 |
| tokei | 371 ms ± 3 | 6.02x ± 0.09 | 5.15 s | 0.42 s | 15.00 | 42.3M | 2.8M | 65,042 | 15,688,680 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 0.0%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 1.1%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0.1% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 61,420 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: t1-fwd: Statistical outliers were detected.
- **hyperfine warning**: t1-rev: Statistical outliers were detected.
- **hyperfine warning**: t2-rev: Statistical outliers were detected.

```
since 20260912-134711 (same machine, same corpus commit)
  mezura  t1   82 ms -> 52 ms   -36.7% ± 1.0%   version v3.1.1 (2026-09-11) -> v3.2.0 (2026-09-19)
  scc     t1   114 ms -> 115 ms   +0.3% ± 2.3%   within the noise
  tokei   t1   304 ms -> 304 ms   -0.2% ± 0.3%   within the noise
  control mezura   timed under another build in 20260912-134711, so the machine's own shift is not known
  differs background 0% busy -> 0.1% busy
          linebench  0.1.0 -> 0.2.0
          drift      2.0% -> 0.0%
```

### cpython corpus, 20260919-180321

measured 2026-09-19 18:03 UTC by linebench 0.2.0  
corpus at `34439b8a2` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.2.0 (2026-09-19), scc 4.1.0, tokei 15.0.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 10 ms ± 1 | 1.00x | 0.08 s | 0.01 s | 9.13 | 216.1M | 23.7M | 3,557 | 2,230,162 |
| scc | 20 ms ± 0 | 1.94x ± 0.14 | 0.22 s | 0.03 s | 12.26 | 111.3M | 9.1M | 3,554 | 2,229,543 |
| tokei | 45 ms ± 1 | 4.40x ± 0.32 | 0.61 s | 0.03 s | 14.25 | 49.1M | 3.4M | 3,581 | 2,230,903 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura | 12 ms ± 1 | 1.00x | 0.09 s | 0.02 s | 9.22 | 182.6M | 19.8M | 3,573 | 2,124,279 |
| scc | 31 ms ± 0 | 2.67x ± 0.16 | 0.36 s | 0.04 s | 12.97 | 98.1M | 7.6M | 5,777 | 3,047,694 |
| tokei | 55 ms ± 2 | 4.72x ± 0.33 | 0.73 s | 0.04 s | 14.01 | 54.7M | 3.9M | 5,657 | 2,998,673 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 0.2%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 3.0%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0.1% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 3,556 files the corpus declares, and the line counts within 1.0% of each other.
- **hyperfine warning**: t2-rev: Statistical outliers were detected.

```
since 20260912-134614 (same machine, same corpus commit)
  mezura  t1   17 ms -> 10 ms   -37.9% ± 1.3%   version v3.1.1 (2026-09-11) -> v3.2.0 (2026-09-19)
  scc     t1   20 ms -> 20 ms   -0.2% ± 0.5%   within the noise
  tokei   t1   45 ms -> 45 ms   -0.1% ± 0.6%   within the noise
  control mezura   timed under another build in 20260912-134614, so the machine's own shift is not known
  differs background 0% busy -> 0.1% busy
          linebench  0.1.0 -> 0.2.0
          drift      15.4% -> 0.2%
```

## Every run

Same-work times, the sections above show only the newest run per machine and corpus. A column whose runs measured different versions of the counter says which beside each time. Commits, machine state and everything else: inside each run's directory.

| run | platform | corpus | mezura | scc | tokei | machine steadiness |
|---|---|---|---|---|---|---|
| [20260919-180551](linux/linux/20260919-180551/) | Native Linux | linux | 86 ms (v3.2.0 (2026-09-19)) | 208 ms | 414 ms | 2.4% |
| [20260919-180418](jdk/linux/20260919-180418/) | Native Linux | jdk | 52 ms (v3.2.0 (2026-09-19)) | 115 ms | 304 ms | 0.0% |
| [20260919-180321](cpython/linux/20260919-180321/) | Native Linux | cpython | 10 ms (v3.2.0 (2026-09-19)) | 20 ms | 45 ms | 0.2% |
| [20260912-134847](linux/linux/20260912-134847/) | Native Linux | linux | 166 ms (v3.1.1 (2026-09-11)) | 210 ms | 414 ms | 0.9% |
| [20260912-134711](jdk/linux/20260912-134711/) | Native Linux | jdk | 82 ms (v3.1.1 (2026-09-11)) | 114 ms | 304 ms | 2.0% |
| [20260912-134614](cpython/linux/20260912-134614/) | Native Linux | cpython | 17 ms (v3.1.1 (2026-09-11)) | 20 ms | 45 ms | 15.4% |

## Local builds, Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

Runs holding an instance given by hand, a build of your own or a release carrying arguments of its own. They compare one build with another on this machine and say nothing about the released counters.

### linux corpus, 20260919-005226

measured 2026-09-19 00:52 UTC by linebench 0.1.0  
corpus at `0ff41df1c` on ext4 /dev/nvme0n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura@dev-v3.2.0 v3.2.0 (unreleased) (local build dev-v3.2.0), scc 4.1.0  
3 warmups, 30 timed runs per command (15 in the first pass + 15 in the reverse pass), 3 s of pause before each command

#### Same work (every counter pinned to the same languages and settings)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura@dev-v3.2.0 | 96 ms ± 4 | 1.00x | 1.05 s | 0.25 s | 13.56 | 375.3M | 27.7M | 63,738 | 36,018,801 |
| scc | 208 ms ± 3 | 2.17x ± 0.09 | 2.77 s | 0.35 s | 15.00 | 172.8M | 11.5M | 63,738 | 36,018,801 |

#### Out of the box (each counter at its own defaults)

| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |
|---|---|---|---|---|---|---|---|---|---|
| mezura@dev-v3.2.0 | 118 ms ± 4 | 1.00x | 1.36 s | 0.27 s | 13.87 | 303.9M | 21.9M | 66,539 | 35,816,820 |
| scc | 321 ms ± 2 | 2.72x ± 0.09 | 4.46 s | 0.44 s | 15.27 | 124.8M | 8.2M | 83,784 | 39,993,947 |

Trust checks for this run:
- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by 0.1%.
- **Command order**: every table ran in both command orders and the numbers above pool the two. Swapping the order moved no counter by more than 1.8%.
- **Power**: set for the run and restored after: cpu governor on 16 cpus: powersave -> performance.
- **Quiet machine**: everything other than the benchmark was using 0.1% of the cpu when the run started, about 0.0 of 16 cores.
- **Equal work**: every file count sat within 1.0% of the 63,779 files the corpus declares, and the line counts within 1.0% of each other.

```
since 20260912-134847 (same machine, same corpus commit, same builds)
  mezura@dev-v3.2.0 t1   96 ms   never measured before on this machine
  scc               t1   210 ms -> 208 ms   -0.8% ± 0.5%
  no earlier run shares this run's control, so the machine's own shift is not known
  differs           background 0% busy -> 0.1% busy
                    drift      0.9% -> 0.1%
  measured in the runs above and not in this run   mezura, tokei
```

## Every local run

Same-work times, the sections above show only the newest run per machine and corpus. A column whose runs measured different versions of the counter says which beside each time. Commits, machine state and everything else: inside each run's directory.

| run | platform | corpus | mezura | mezura@dev-v3.2.0 | mezura@v3.1.0 | scc | machine steadiness |
|---|---|---|---|---|---|---|---|
| [20260919-005226](local/linux/linux/20260919-005226/) | Native Linux | linux |  | 96 ms |  | 208 ms | 0.1% |
| [20260906-220447](local/linux/linux/20260906-220447/) | Native Linux | linux | 224 ms |  | 167 ms |  | 0.2% |

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
