| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/mezura /tmp/linebench-insights-20260913-011907/floor --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 4.0 ± 0.1 | 3.9 | 4.2 | 1.84 ± 0.40 |
| `/home/petros/Documents/dev/tools/scc /tmp/linebench-insights-20260913-011907/floor -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 2.9 ± 0.2 | 2.7 | 3.3 | 1.35 ± 0.30 |
| `/home/petros/Documents/dev/tools/tokei /tmp/linebench-insights-20260913-011907/floor -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 2.2 ± 0.5 | 1.8 | 3.1 | 1.00 |
| `/home/petros/Documents/dev/tools/cloc.pl /tmp/linebench-insights-20260913-011907/floor --include-ext=c,C,h,H,s,S,asm,ASM,py,PY,pl,PL,pm,PM,rs,RS,sh,SH --skip-uniqueness` | 43.3 ± 0.9 | 42.1 | 45.4 | 20.03 ± 4.39 |
