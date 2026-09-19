| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/mezura /tmp/linebench-insights-20260919-181010/floor --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 3.8 ± 0.2 | 3.5 | 4.2 | 1.70 ± 0.38 |
| `/home/petros/Documents/dev/tools/scc /tmp/linebench-insights-20260919-181010/floor -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 2.9 ± 0.2 | 2.6 | 3.6 | 1.30 ± 0.31 |
| `/home/petros/Documents/dev/tools/tokei /tmp/linebench-insights-20260919-181010/floor -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 2.2 ± 0.5 | 1.8 | 2.9 | 1.00 |
