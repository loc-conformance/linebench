| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/cs /home/petros/Documents/dev/bench/linux --ext c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --hidden --include-generated --no-gitattributes --no-config` | 79.5 ± 1.6 | 77.9 | 82.8 | 1.20 ± 0.04 |
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords,tests --count-minified --count-generated --count-not-code --counting region --no-shebang` | 66.0 ± 1.7 | 62.9 | 68.0 | 1.00 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 208.5 ± 2.7 | 205.6 | 214.6 | 3.16 ± 0.09 |
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/linux -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 414.6 ± 4.8 | 407.2 | 424.3 | 6.28 ± 0.17 |
