| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 86.8 ± 3.7 | 79.9 | 93.0 | 1.00 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 207.4 ± 2.4 | 203.5 | 213.2 | 2.39 ± 0.10 |
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/linux -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 413.4 ± 2.5 | 409.8 | 417.1 | 4.76 ± 0.20 |
