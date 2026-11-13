| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/cs /home/petros/Documents/dev/bench/linux --ext c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --hidden --include-generated --no-gitattributes --no-config` | 672.2 ± 0.9 | 671.4 | 673.1 | 1.32 ± 0.00 |
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords,tests --count-minified --count-generated --count-not-code --counting region --no-shebang` | 508.3 ± 0.4 | 507.8 | 508.7 | 1.00 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 1622.6 ± 19.4 | 1600.7 | 1637.8 | 3.19 ± 0.04 |
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/linux -t "C,C Header,GNU Style Assembly,Assembly,Python,Perl,Perl,Rust,Shell" --no-ignore --hidden` | 2942.0 ± 10.7 | 2934.5 | 2954.3 | 5.79 ± 0.02 |
