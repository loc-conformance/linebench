# Insights

Written by `linebench insights` when the session ends. What a run cannot measure about itself, since watching a process closely enough disturbs the times it would report. The tables are the ones the command printed, kept as they were laid out.

## Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

measured 2026-11-13 03:36 UTC by linebench 0.4.1  
linux corpus at `0ff41df1c` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
codestats 1.1.0, mezura v3.5.0 (2026-11-12), scc 4.1.0, tokei 15.0.0

## Floor

What a counter costs before it has counted anything.

```
   instance   --version     ready t1 (same work)  ready t2 (out of the box)
   codestats  0.3 ms ± 0.1  0.6 ms ± 0.2          0.6 ms ± 0.2
   mezura     0.4 ms ± 0.1  2.2 ms ± 1.2          1.8 ms ± 0.1
   scc        2.8 ms ± 0.2  2.9 ms ± 0.2          2.9 ms ± 0.2
   tokei      0.4 ms ± 0.1  2.1 ms ± 0.5          2.2 ms ± 0.4

   --version       the binary answering its version flag and quitting
   ready t1, t2    the same binary over a target with no files, its report printed
```

## Memory

What each counter held while it counted, sampled while it ran. The axis under each curve is the wall time of that run, and the pages are what the run was given, its own buffers along with the corpus it read.

```
   codestats   peak 104 MB   15,813 pages   36 samples, 2.2 ms apart
   200 MB │ 
          │ 
   133 MB │                                                           ▁
          │                                                           █
    67 MB │                                                     ▃ ▃ ▃ █
          │   ▂ ▃ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▃ ▃ ▂ ▃ ▃ ▂ ▃ ▃ ▃ ▃ ▃ ▆ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            18            37            56             78 ms

   mezura   peak 112 MB   13,663 pages   25 samples, 2.7 ms apart
   the execution was too short to draw, it wants 30

   scc   peak 359 MB   1,866 pages   92 samples, 2.2 ms apart
   500 MB │ 
          │                                           ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂
   333 MB │                                     ▂ ▅ █ █ █ █ █ █ █ █ █ █
          │                                   ▄ █ █ █ █ █ █ █ █ █ █ █ █
   167 MB │                               ▁ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ ▄ ▅ ▅ ▆ ▆ ▇ ▇ ▇ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            49            99            149           207 ms

   tokei   peak 226 MB   45,473 pages   190 samples, 2.1 ms apart
   500 MB │ 
          │ 
   333 MB │ 
          │                                         ▄ ▃ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄
   167 MB │                                       ▅ █ █ █ █ █ █ █ █ █ █
          │ ▃ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ █ █ █ █ █ █ █ █ █ █ █
        0 └─┬─────────────┬─────────────┬─────────────┬───────────────┬
            0            98            196           294           406 ms
```

## System calls

What each counter asked of the kernel, counted by the tracer.

```
   instance   syscalls  per file  errors
   codestats   291,032       4.6     187
   mezura      233,411       3.7     486
   scc         378,711       5.9   6,405
   tokei       934,303      14.6   1,776

   family / call  codestats   mezura      scc    tokei
   directories       11,792   11,800   11,848   11,814
     getdents64      11,792   11,800   11,848   11,814
   opening          139,292  139,498  139,273  147,284
     openat          69,647   69,751   69,637   73,650
     close           69,645   69,747   69,636   73,634
   metadata          72,261    9,003   63,780    5,943
     statx           66,354    3,099                32
     newfstatat                        63,780
     fstat            5,900    5,903             5,910
     rest (2)             7        1                 1
   reading           61,177   66,232  127,454  759,931
     read            61,175   66,230  127,454  759,929
     pread64              2        2                 2
   memory             5,602    1,215      129    1,715
   threads               80      271      557      149
   waiting              598    4,911    4,974    7,206
     sched_yield               4,374      570    1,077
     rest (5)           598      537    4,404    6,129
   other                230      481   30,696      261
     fcntl                             23,586
     epoll_ctl                          5,896
     rest (18)          230      481    1,214      261
```

## Hardware counters

What the cpu did while each counter counted, read through perf in three passes of four events.

```
   instance                  codestats      mezura         scc       tokei
   lines                    36,018,801  36,019,013  36,018,801  36,027,518
   work
     instructions               10.7 G      8.89 G      39.1 G      63.4 G
     per line                      298         247       1,085       1,759
     cycles                     5.95 G      4.38 G      14.9 G      29.4 G
     per line                      165         122         414         817
     insn per cycle               1.81        2.03        2.62        2.15
   branches
     branch-instructions        1.89 G      1.51 G      9.12 G      14.7 G
     branch-misses              40.4 M      18.7 M      70.0 M       101 M
     per 1k instructions          3.76        2.11        1.79        1.59
     per line                     1.12        0.52        1.94        2.79
   memory
     L1-dcache-loads            3.59 G      3.62 G      15.5 G      26.6 G
     L1-dcache-load-misses       102 M      89.8 M       139 M       187 M
     per 1k loads                 28.3        24.8        8.94        7.04
     per line                     2.83        2.49        3.85        5.20
     cache-references            306 M       240 M       381 M       429 M
     cache-misses               11.3 M      9.56 M      30.6 M      30.9 M
     per 1k refs                  36.8        39.8        80.2        72.1
     per line                     0.31        0.27        0.85        0.86
     dTLB-load-misses           1.37 M     599,222     219,068     684,242
     iTLB-load-misses          495,084     115,993      36,121      39,722
   system
     page-faults                16,248      13,061       1,946      44,861
     context-switches              546         621       1,878       5,001

   M is a million, G is a billion, T is a trillion
   every event counted for the whole of every run
   over 5 runs the instructions of an instance moved at most 0.15% and its cycles 0.73%
   cache-references and cache-misses name a different level of cache on each processor
```

## Allocations

What each counter asked of the heap allocator, counted by valgrind.

```
   instance      allocs   floor  per file  allocated  per alloc  at exit
   codestats    546,427     781       8.6     257 MB      494 B   460 KB
   mezura       381,099  18,770       5.7     217 MB      598 B    19 KB
   scc
   tokei      1,508,870     767      23.6     4.5 GB     3.1 KB   508 KB

   scc made no allocations through libc, so valgrind saw none of its heap
```

## Affinity ladder

How each counter scales as physical cores and hardware threads are added through the process affinity mask.

```
   instance        codestats       mezura          scc        tokei
   1c
     median         671.9 ms     508.3 ms    1629.3 ms    2937.2 ms
   2c
     median         342.1 ms     260.3 ms     847.0 ms    1495.8 ms
     speedup           1.96x        1.95x        1.92x        1.96x
     from prev         1.96x        1.95x        1.92x        1.96x
     efficiency          98%          98%          96%          98%
   4c
     median         176.2 ms     136.5 ms     422.8 ms     768.2 ms
     speedup           3.81x        3.72x        3.85x        3.82x
     from prev         1.94x        1.91x        2.00x        1.95x
     efficiency          95%          93%          96%          96%
   8c
     median         103.1 ms      82.0 ms     238.2 ms     453.0 ms
     speedup           6.52x        6.20x        6.84x        6.48x
     from prev         1.71x        1.66x        1.78x        1.70x
     efficiency          81%          77%          86%          81%
   16t
     median          79.1 ms      65.8 ms     208.2 ms     414.2 ms
     speedup           8.50x        7.72x        7.82x        7.09x
     from prev         1.30x        1.25x        1.14x        1.09x
   summary
     peak        8.50x (16t)  7.72x (16t)  7.82x (16t)  7.09x (16t)
     smt gain         +30.3%       +24.6%       +14.4%        +9.4%

   8 physical cores, 16 hardware threads
   speedup is against 1c, from prev is against the rung above, and efficiency is speedup divided by physical cores
```

## Cold cache

How long each counter takes when its binary and the corpus come from the disk, as after a reboot. Shared libraries that running programs hold mapped, libc among them, stay in memory, as they do after a reboot too. The cold times hold for the disk of this machine.

```
   instance        cold               spread      warm  cold / warm  from disk
   codestats  1383.9 ms  1370.8 to 1396.4 ms   80.5 ms       17.19x     1.5 GB
   mezura     1222.2 ms  1199.4 to 1239.1 ms   69.2 ms       17.66x     1.5 GB
   scc        1433.7 ms  1412.7 to 1463.7 ms  210.8 ms        6.80x     1.5 GB
   tokei      1800.8 ms  1792.7 to 1802.6 ms  412.9 ms        4.36x     1.5 GB

   each cold run follows an emptied page cache, the counters take turns in rounds of alternating order, and the first two rounds of cold runs and of warm runs are thrown away; from disk is what the device gave, readahead included
```
