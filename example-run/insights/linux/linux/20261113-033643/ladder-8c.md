| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/cs /home/petros/Documents/dev/bench/linux --ext c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --hidden --include-generated --no-gitattributes --no-config` | 102.6 ± 1.1 | 100.8 | 103.5 | 1.26 ± 0.05 |
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords,tests --count-minified --count-generated --count-not-code --counting region --no-shebang` | 81.7 ± 3.0 | 78.5 | 86.2 | 1.00 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 239.8 ± 4.3 | 237.1 | 247.4 | 2.93 ± 0.12 |
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/linux -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 452.7 ± 5.0 | 448.1 | 460.6 | 5.54 ± 0.21 |
