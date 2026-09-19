| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `D:/dev/tools/linebench-counters/mezura.exe D:/dev/bench-corpora/jdk --languages java,cpp,cc,hpp,hh,c,h,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 139.6 ± 7.8 | 135.5 | 165.3 | 1.00 |
| `D:/dev/tools/linebench-counters/scc.exe D:/dev/bench-corpora/jdk -i java,cpp,cc,hpp,hh,c,h,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 279.2 ± 16.5 | 268.8 | 311.9 | 2.00 ± 0.16 |
| `D:/dev/tools/linebench-counters/tokei.exe D:/dev/bench-corpora/jdk -t "Java,C++,C++,C++ Header,C++ Header,C,C Header,Shell" --no-ignore --hidden` | 442.2 ± 19.0 | 425.3 | 470.4 | 3.17 ± 0.22 |
