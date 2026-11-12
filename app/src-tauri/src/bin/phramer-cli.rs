//! `phramer-cli.exe`: Phramer's command line with a console, so `--raw`,
//! `--print-geometry`, `--help` and errors reach the terminal that ran it.
//! A console-subsystem binary; `phramer.exe` has no console.

fn main() {
    std::process::exit(phramer_lib::run_cli())
}
