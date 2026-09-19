# Insights

Written by `linebench insights` when the session ends. What a run cannot measure about itself, since watching a process closely enough disturbs the times it would report. The tables are the ones the command printed, kept as they were laid out.

## Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

measured 2026-09-19 01:29 UTC by linebench 0.1.0  
linux corpus at `0ff41df1c` on ext4 /dev/nvme0n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura@dev-v3.2.0 v3.2.0 (unreleased) (local build dev-v3.2.0), scc 4.1.0

## Floor

What a counter costs before it has counted anything.

```
   instance           --version     ready t1 (same work)  ready t2 (out of the box)
   mezura@dev-v3.2.0  0.7 ms ± 0.0  3.8 ms ± 0.1          3.8 ms ± 0.1
   scc                2.7 ms ± 0.2  3.0 ms ± 0.2          3.0 ms ± 0.2

   --version       the binary answering its version flag and quitting
   ready t1, t2    the same binary over a target with no files, its report printed
```

## Memory

What each counter held while it counted, sampled while it ran. The axis under each curve is the wall time of that run, and the pages are what the run was given, its own buffers along with the corpus it read.

```
   mezura@dev-v3.2.0   peak 78 MB   14,225 pages   42 samples, 2.3 ms apart
   100 MB │ 
          │                                                     ▅ ▃
    67 MB │                                             █ ▇ ▇ ▇ █ █
          │                                             █ █ █ █ █ █ ▅
    33 MB │       ▁ ▂ ▃ ▃ ▄ ▄ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▆ ▆ █ █ █ █ █ █ █ ▅
          │   ▄ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            23            47            71             99 ms

   scc   peak 341 MB   1,838 pages   98 samples, 2.1 ms apart
   500 MB │ 
          │                                       ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁
   333 MB │                                     █ █ █ █ █ █ █ █ █ █ █ █
          │                                 ▄ ▇ █ █ █ █ █ █ █ █ █ █ █ █
   167 MB │                             ▁ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ ▄ ▄ ▅ ▆ ▆ ▆ ▇ ▇ ▇ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            49            99            149           207 ms
```

## System calls

What each counter asked of the kernel, counted by the tracer.

```
   instance           syscalls  per file  errors
   mezura@dev-v3.2.0   235,299       3.7     536
   scc                 377,708       5.9   6,306

   family / call  mezura@dev-v3.2.0      scc
   directories               11,800   11,848
     getdents64              11,800   11,848
   opening                  139,458  139,273
     close                   69,729   69,636
     openat                  69,729   69,637
   metadata                   8,982   63,780
     newfstatat                       63,780
     fstat                    5,903
     statx                    3,078
     access                       1
   reading                   66,202  127,454
     read                    66,200  127,454
     pread64                      2
   memory                     1,329      126
   threads                      226      526
   waiting                    6,837    4,068
     sched_yield              4,561      517
     rest (5)                 2,276    3,551
   other                        465   30,633
     fcntl                            23,586
     epoll_ctl                         5,896
     rest (15)                  465    1,151
```

## Hardware counters

What the cpu did while each counter counted, read through perf in three passes of four events.

```
   instance                 mezura@dev-v3.2.0         scc
   lines                           36,018,801  36,018,801
   work
     instructions                      13.8 G      39.2 G
     per line                             383       1,087
     cycles                            6.21 G      15.0 G
     per line                             172         415
     insn per cycle                      2.22        2.62
   branches
     branch-instructions               2.60 G      9.13 G
     branch-misses                     42.6 M      70.4 M
     per 1k instructions                 3.09        1.80
     per line                            1.18        1.95
   memory
     L1-dcache-loads                   5.92 G      15.5 G
     L1-dcache-load-misses             78.6 M       138 M
     per 1k loads                        13.3        8.90
     per line                            2.18        3.82
     cache-references                   235 M       378 M
     cache-misses                      10.5 M      29.7 M
     per 1k refs                         44.6        78.5
     per line                            0.29        0.82
     dTLB-load-misses                 608,963     216,694
     iTLB-load-misses                 171,003      38,002
   system
     page-faults                       14,040       2,055
     context-switches                   1,015       1,426

   M is a million, G is a billion, T is a trillion
   nothing was multiplexed, every event counted for the whole of every run
   over 5 runs the instructions of an instance moved at most 0.18% and its cycles 0.56%
   cache-references and cache-misses are whatever this kernel maps them to,
   so they compare inside this table alone
   read as root, so the events opened with kernel.perf_event_paranoid at 3
```
