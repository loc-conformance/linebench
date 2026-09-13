# Insights

Written by `linebench insights` when the session ends. What a run cannot measure about itself, since watching a process closely enough disturbs the times it would report. The tables are the ones the command printed, kept as they were laid out.

## Native Linux, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 60 GB usable RAM, Debian GNU/Linux 13 (trixie)

measured 2026-09-13 01:19 UTC by linebench 0.1.0  
linux corpus at `0ff41df1c` on ext4 /dev/nvme1n1p3, Lexar SSD NQ790 2TB, 16.0 GT/s PCIe x4  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0, cloc 2.10

## Floor

What a counter costs before it has counted anything.

```
   instance  --version      ready t1 (same work)  ready t2 (out of the box)
   mezura    0.8 ms ± 0.0   4.0 ms ± 0.1          4.0 ms ± 0.1
   scc       2.8 ms ± 0.3   2.9 ms ± 0.2          3.1 ms ± 0.2
   tokei     0.4 ms ± 0.1   2.2 ms ± 0.5          2.3 ms ± 0.5
   cloc      40.3 ms ± 0.4  43.3 ms ± 0.9         43.5 ms ± 1.1

   --version       the binary answering its version flag and quitting
   ready t1, t2    the same binary over a target with no files, its report printed
```

## Memory

What each counter held while it counted, sampled while it ran. The axis under each curve is the wall time of that run.

```
   mezura   peak 107 MB   73 samples, 2.4 ms apart
   200 MB │ 
          │ 
   133 MB │                                                   ▂ ▂   ▁ ▁
          │                                           ▁ ▇ ▅ ▃ █ █ █ █ █
    67 MB │                           ▁ ▁ ▁ ▁ ▁ ▂ ▃ ▃ █ █ █ █ █ █ █ █ █
          │ ▂ ▃ ▄ ▅ ▆ ▇ ▇ ▇ ▇ ▇ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0            44             89             133          178 ms

   scc   peak 391 MB   100 samples, 2.2 ms apart
   500 MB │ 
          │                                               ▅ ▆ ▆ ▆ ▆ ▆ ▆
   333 MB │                                         ▂ ▄ ▇ █ █ █ █ █ █ █
          │                                         █ █ █ █ █ █ █ █ █ █
   167 MB │                               ▁ ▁ ▁ ▃ █ █ █ █ █ █ █ █ █ █ █
          │ ▃ ▄ ▅ ▆ ▇ ▇ ▇ ▇ ▇ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0            55             110            165          221 ms

   tokei   peak 251 MB   197 samples, 2.2 ms apart
   500 MB │ 
          │ 
   333 MB │ 
          │                                       ▁ ▄ █ █ ▄ ▄ ▄ ▄ ▄ ▄ ▅
   167 MB │                                       █ █ █ █ █ █ █ █ █ █ █
          │ ▃ ▃ ▃ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▄ ▆ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0            108            217            325          434 ms

   cloc   peak 337 MB   25125 samples, 2.1 ms apart
   500 MB │ 
          │ 
   333 MB │               █ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆ ▆
          │               █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
   167 MB │   ▁ ▂ ▄ ▃ ▃ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ ▆ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0           12946          25892          38838        51784 ms
```

## System calls

What each counter asked of the kernel, counted by the tracer.

```
   instance   syscalls  per file   errors
   mezura      374,525       5.9      512
   scc         378,220       5.9    6,400
   tokei       938,234      14.7    3,031
   cloc      3,021,837      47.4  239,278

   family / call       mezura      scc    tokei       cloc
   directories         11,800   11,848   11,814     11,798
     getdents64        11,800   11,848   11,814     11,798
   opening            139,458  139,273  147,289    477,199
     openat            69,729   69,637   73,655    238,615
     close             69,729   69,636   73,634    238,584
   metadata            69,838   63,780    5,943    622,506
     newfstatat                 63,780             383,980
     fstat              5,903             5,910    238,522
     statx             63,934                32
     rest (2)               1                 1          4
   reading             63,920  127,454  759,936    454,826
     read              63,918  127,454  759,934    454,820
     rest (1)               2                 2          6
   memory               1,076      125    1,674      1,590
   threads                406      566      149          8
   waiting             87,178    4,460   11,168          3
     sched_yield       66,831      667    1,110
     clock_nanosleep   18,628                12
     futex              1,718    2,810   10,045          3
     rest (3)               1      983        1
   other                  849   30,714      261  1,453,907
     lseek                                   25    527,666
     rt_sigprocmask       451       62      131    335,879
     ioctl                  5                 2    232,615
     rt_sigaction           6      114        6    168,005
     alarm                                         167,936
     fcntl                      23,586                   7
     epoll_ctl                   5,896
     rest (25)            387    1,056       97     21,799
```
