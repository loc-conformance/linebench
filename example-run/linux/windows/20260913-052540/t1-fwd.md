| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `D:/dev/tools/linebench-counters/mezura.exe D:/dev/bench-corpora/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 260.3 ± 25.8 | 233.2 | 326.1 | 1.00 |
| `D:/dev/tools/linebench-counters/scc.exe D:/dev/bench-corpora/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 521.4 ± 31.9 | 462.2 | 580.8 | 2.00 ± 0.23 |
| `D:/dev/tools/linebench-counters/tokei.exe D:/dev/bench-corpora/linux -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 627.4 ± 22.0 | 591.2 | 662.3 | 2.41 ± 0.25 |
