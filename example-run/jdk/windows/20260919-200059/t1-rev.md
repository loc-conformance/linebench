| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `D:/dev/tools/linebench-counters/tokei.exe D:/dev/bench-corpora/jdk -t "Java,C++,C++,C++ Header,C++ Header,C,C Header,Shell" --no-ignore --hidden` | 453.6 ± 25.1 | 426.2 | 489.9 | 3.24 ± 0.25 |
| `D:/dev/tools/linebench-counters/scc.exe D:/dev/bench-corpora/jdk -i java,cpp,cc,hpp,hh,c,h,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 280.2 ± 16.2 | 266.7 | 313.8 | 2.00 ± 0.16 |
| `D:/dev/tools/linebench-counters/mezura.exe D:/dev/bench-corpora/jdk --languages java,cpp,cc,hpp,hh,c,h,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 140.0 ± 7.5 | 136.3 | 160.2 | 1.00 |
