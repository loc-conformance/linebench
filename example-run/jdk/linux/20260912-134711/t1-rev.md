| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `/home/petros/Documents/dev/tools/tokei /home/petros/Documents/dev/bench/jdk -t "Java,C++,C++,C++ Header,C++ Header,C,C Header,Shell" --no-ignore --hidden` | 304.6 ± 2.9 | 299.4 | 309.2 | 3.69 ± 0.21 |
| `/home/petros/Documents/dev/tools/scc /home/petros/Documents/dev/bench/jdk -i java,cpp,cc,hpp,hh,c,h,sh --no-gitignore --no-ignore --no-scc-ignore -c --no-cocomo --no-config` | 113.8 ± 7.4 | 110.6 | 140.0 | 1.38 ± 0.12 |
| `/home/petros/Documents/dev/tools/mezura /home/petros/Documents/dev/bench/jdk --languages java,cpp,cc,hpp,hh,c,h,sh --no-gitignore --no-ignore-files --search-in-dotted --no-local --no-default-config --hide keywords --count-minified --count-generated --count-not-code --counting region --no-shebang` | 82.6 ± 4.6 | 79.6 | 95.0 | 1.00 |
