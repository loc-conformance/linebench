# Insights

Written by `linebench insights` when the session ends. What a run cannot measure about itself, since watching a process closely enough disturbs the times it would report. The tables are the ones the command printed, kept as they were laid out.

## Windows, AMD Ryzen 7 9700X 8-Core Processor

16 threads, 62 GB usable RAM, Microsoft Windows 11 Pro

measured 2026-09-13 05:35 UTC by linebench 0.1.0  
linux corpus at `0ff41df1c` on NTFS, Lexar SSD NQ790 2TB, SSD, NVMe  
mezura v3.1.1 (2026-09-11), scc 4.1.0, tokei 15.0.0, cloc 2.10

## Floor

What a counter costs before it has counted anything.

```
   instance  --version       ready t1 (same work)  ready t2 (out of the box)
   mezura    6.1 ms ± 0.1    11.0 ms ± 0.1         11.1 ms ± 0.2
   scc       12.8 ms ± 0.5   13.1 ms ± 0.4         13.1 ms ± 0.4
   tokei     4.2 ms ± 0.1    9.3 ms ± 0.9          9.7 ms ± 0.9
   cloc      133.1 ms ± 0.9  137.9 ms ± 0.9        137.7 ms ± 0.8

   --version       the binary answering its version flag and quitting
   ready t1, t2    the same binary over a target with no files, its report printed
```

## Memory

What each counter held while it counted, sampled while it ran. The axis under each curve is the wall time of that run.

```
   mezura   peak 91 MB   96 samples, 2.5 ms apart
   100 MB │                                 ▁ ▄ ▁ ▂     ▁ ▁
          │                               █ █ █ █ █ ▅ ▄ █ █
    67 MB │                               █ █ █ █ █ █ █ █ █ ▆ ▆ ▅ ▄   ▁
          │                 ▁ ▁ ▁ ▁ ▁ ▁ ▂ █ █ █ █ █ █ █ █ █ █ █ █ █ ▄ █
    33 MB │   ▁ ▂ ▃ ▅ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ ▃ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0            60             120            180          241 ms

   scc   peak 358 MB   199 samples, 2.4 ms apart
   500 MB │ 
          │                           ▁ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂ ▂
   333 MB │                         ▄ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │                       ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
   167 MB │                     ▁ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ ▁ ▄ ▆ ▇ ▇ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0            118            236            354          473 ms

   tokei   peak 190 MB   213 samples, 3.0 ms apart
   200 MB │                                         ▃ ▄
          │                                     ▁ ▃ █ █
   133 MB │                                     █ █ █ █
          │                                     █ █ █ █ ▆
    67 MB │   ▃ ▄ ▄ ▅ ▅ ▄ ▄ ▄ ▄ ▄ ▃ ▃ ▃ ▃ ▃ ▃ ▅ █ █ █ █ █ ▂ ▂ ▂ ▁ ▂ ▁ ▂
          │ ▇ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0            162            325            488          651 ms

   cloc   peak 7.4 MB   30086 samples, 2.5 ms apart
    10 MB │ 
          │ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁ ▁
   6.7 MB │ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
   3.3 MB │ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
          │ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █
        0 └┬─────────────┬──────────────┬──────────────┬──────────────┬
           0           18967          37935          56903        75871 ms
```

## System calls

strace runs on linux alone, so the system calls cannot be measured here.
