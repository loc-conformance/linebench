| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/cs /tmp/linebench-insights-20261113-033643/floor --ext c,h,s,asm,py,pl,pm,rs,sh` | 0.6 ± 0.2 | 0.4 | 0.9 | 1.00 |
| `/home/petros/Documents/dev/tools/mezura /tmp/linebench-insights-20261113-033643/floor --languages c,h,s,asm,py,pl,pm,rs,sh` | 1.8 ± 0.1 | 1.7 | 1.9 | 2.90 ± 0.78 |
| `/home/petros/Documents/dev/tools/scc /tmp/linebench-insights-20261113-033643/floor -i c,h,s,asm,py,pl,pm,rs,sh` | 2.9 ± 0.2 | 2.6 | 3.3 | 4.73 ± 1.30 |
| `/home/petros/Documents/dev/tools/tokei /tmp/linebench-insights-20261113-033643/floor -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell"` | 2.2 ± 0.4 | 1.9 | 3.1 | 3.55 ± 1.16 |
