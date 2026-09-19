| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/linux -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 413.9 ± 2.4 | 409.7 | 417.6 | 4.83 ± 0.12 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 208.1 ± 1.6 | 205.7 | 210.6 | 2.43 ± 0.06 |
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 85.6 ± 2.1 | 82.7 | 90.9 | 1.00 |
