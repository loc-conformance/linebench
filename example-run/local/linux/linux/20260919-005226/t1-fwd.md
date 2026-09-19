| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/given/mezura@dev-v3.2.0/mezura /home/petros/Documents/dev/bench/linux --languages c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 95.8 ± 3.4 | 90.7 | 101.5 | 1.00 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/linux -i c,h,s,asm,py,pl,pm,rs,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 209.1 ± 3.7 | 205.0 | 217.0 | 2.18 ± 0.09 |
