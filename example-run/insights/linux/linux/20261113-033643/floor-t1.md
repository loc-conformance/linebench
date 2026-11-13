| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/cs /tmp/linebench-insights-20261113-033643/floor --ext c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --hidden --include-generated --no-gitattributes --no-config` | 0.6 ± 0.2 | 0.4 | 0.8 | 1.00 |
| `/home/petros/Documents/dev/tools/mezura /tmp/linebench-insights-20261113-033643/floor --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords,tests --count-minified --count-generated --count-not-code --counting region --no-shebang` | 2.2 ± 1.2 | 1.7 | 7.8 | 3.81 ± 2.31 |
| `/home/petros/Documents/dev/tools/scc /tmp/linebench-insights-20261113-033643/floor -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 2.9 ± 0.2 | 2.6 | 3.3 | 5.08 ± 1.38 |
| `/home/petros/Documents/dev/tools/tokei /tmp/linebench-insights-20261113-033643/floor -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 2.1 ± 0.5 | 1.8 | 2.9 | 3.66 ± 1.26 |
