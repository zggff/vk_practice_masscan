# masscan wrapper in rust

async cli tool written in rust, using clap, serde, tokio
accepts ip range, port range, thread count
##  goals
- [x] call masscan, and parse resulting json
- [x] for each found port, get banner
- [ ] on http/https protocol get HEAD
- [x] save/load results to/from file
- [x] compare current results with previuos
- [ ] on change notify users
