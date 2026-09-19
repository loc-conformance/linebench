| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `D:/dev/tools/linebench-counters/tokei.exe D:/dev/bench-corpora/cpython -t "Python,C,C Header,HTML,JavaScript,Batch,Shell,Objective-C" --no-ignore --hidden` | 67.3 ± 3.9 | 64.4 | 80.9 | 2.87 ± 0.17 |
| `D:/dev/tools/linebench-counters/scc.exe D:/dev/bench-corpora/cpython -i py,c,h,html,js,bat,sh,m --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 40.7 ± 0.7 | 39.8 | 41.8 | 1.74 ± 0.04 |
| `D:/dev/tools/linebench-counters/mezura.exe D:/dev/bench-corpora/cpython --languages py,c,h,html,js,bat,sh,m --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 23.4 ± 0.4 | 23.0 | 24.5 | 1.00 |
