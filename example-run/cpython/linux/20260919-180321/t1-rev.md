| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/cpython -t "Python,C,C Header,HTML,JavaScript,Batch,Shell,Objective-C" --no-ignore --hidden` | 45.4 ± 0.9 | 43.9 | 47.5 | 4.42 ± 0.38 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/cpython -i py,c,h,html,js,bat,sh,m --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 20.0 ± 0.3 | 19.5 | 20.8 | 1.95 ± 0.17 |
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/cpython --languages py,c,h,html,js,bat,sh,m --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 10.3 ± 0.9 | 8.6 | 11.6 | 1.00 |
