# Insights

Written by `linebench insights` when the session ends. What a run cannot measure about itself, since watching a process closely enough disturbs the times it would report. The tables are the ones the command printed, kept as they were laid out.

## Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

measured 2026-09-19 18:10 UTC by linebench 0.2.0  
linux corpus at `0ff41df1c` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.2.0 (2026-09-19), scc 4.1.0, tokei 15.0.0

## Floor

What a counter costs before it has counted anything.

```
   instance  --version     ready t1 (same work)  ready t2 (out of the box)
   mezura    0.7 ms ± 0.0  3.8 ms ± 0.2          3.8 ms ± 0.1
   scc       2.8 ms ± 0.2  2.9 ms ± 0.2          3.0 ms ± 0.1
   tokei     0.5 ms ± 0.1  2.2 ms ± 0.5          2.1 ms ± 0.5

   --version       the binary answering its version flag and quitting
   ready t1, t2    the same binary over a target with no files, its report printed
```

## Memory

What each counter held while it counted, sampled while it ran. The axis under each curve is the wall time of that run, and the pages are what the run was given, its own buffers along with the corpus it read.

```
   mezura   peak 69 MB   13,681 pages   32 samples, 2.6 ms apart
   100 MB │ 
          │ 
    67 MB │                                           ▁         ▁ ▂   ▇
          │                                   ▇ █ ▅ ▅ █ ▄ ▃ ▃ ▃ █ █ ▆ █
    33 MB │       ▁ ▁ ▁ ▃ ▃ ▃ ▃ ▃ ▃ ▄ ▄ ▄ ▅ ▅ █ █ █ █ █ █ █ █ █ █ █ █ █
          │   ▆ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            20            40            60             83 ms

   scc   peak 382 MB   3,250 pages   99 samples, 2.2 ms apart
   500 MB │ 
          │                                                       ▅ ▅ ▅
   333 MB │                                                 ▁ ▄ █ █ █ █
          │                                                 █ █ █ █ █ █
   167 MB │                                 ▁ ▁ ▁ ▁ ▁ ▁ ▃ █ █ █ █ █ █ █
          │ ▃ ▄ ▅ ▆ ▆ ▇ ▇ ▇ ▇ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            52            105           157           218 ms

   tokei   peak 266 MB   47,956 pages   193 samples, 2.1 ms apart
   500 MB │ 
          │ 
   333 MB │ 
          │                                                   ▄ ▇ ▇ ▇ ▇
   167 MB │                                                   █ █ █ █ █
          │ ▃ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▅ ▅ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            100           200           301           416 ms
```

## System calls

What each counter asked of the kernel, counted by the tracer.

```
   instance  syscalls  per file  errors
   mezura     234,627       3.7     412
   scc        378,944       5.9   6,430
   tokei      932,464      14.6   1,138

   family / call   mezura      scc    tokei
   directories     11,800   11,848   11,814
     getdents64    11,800   11,848   11,814
   opening        139,458  139,273  147,289
     openat        69,729   69,637   73,655
     close         69,729   69,636   73,634
   metadata         8,982   63,780    5,943
     newfstatat             63,780
     fstat          5,903             5,910
     statx          3,078                32
     access             1                 1
   reading         66,202  127,454  759,936
     read          66,200  127,454  759,934
     pread64            2                 2
   memory           1,287      119    1,630
   threads            226      568      149
   waiting          6,207    5,185    5,442
     sched_yield    4,519      649    1,271
     rest (5)       1,688    4,536    4,171
   other              465   30,717      261
     fcntl                  23,586
     epoll_ctl               5,896
     rest (17)        465    1,235      261
```

## Hardware counters

What the cpu did while each counter counted, read through perf in three passes of four events.

```
   instance                     mezura         scc       tokei
   lines                    36,018,801  36,018,801  36,027,518
   work
     instructions               11.9 G      39.2 G      63.4 G
     per line                      330       1,087       1,759
     cycles                     5.50 G      14.9 G      29.6 G
     per line                      153         413         820
     insn per cycle               2.16        2.63        2.14
   branches
     branch-instructions        2.31 G      9.14 G      14.7 G
     branch-misses              39.2 M      70.3 M       100 M
     per 1k instructions          3.29        1.80        1.58
     per line                     1.09        1.95        2.78
   memory
     L1-dcache-loads            5.15 G      15.5 G      26.7 G
     L1-dcache-load-misses      79.4 M       139 M       188 M
     per 1k loads                 15.4        8.95        7.04
     per line                     2.21        3.85        5.21
     cache-references            223 M       389 M       427 M
     cache-misses               9.61 M      29.6 M      30.6 M
     per 1k refs                  43.0        76.1        71.7
     per line                     0.27        0.82        0.85
     dTLB-load-misses          612,772     242,550     668,206
     iTLB-load-misses          145,248      43,154      46,607
   system
     page-faults                13,870       2,179      48,136
     context-switches            1,715       1,714       4,572

   M is a million, G is a billion, T is a trillion
   every event counted for the whole of every run
   over 5 runs the instructions of an instance moved at most 0.12% and its cycles 0.42%
   cache-references and cache-misses name a different level of cache on each processor
   read as root, so the events opened with kernel.perf_event_paranoid at 3
```
