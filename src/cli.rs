//! CLI entrypoints for dashcam-archive.

use clap::CommandFactory;

use crate::Cli;

pub fn print_help_and_exit() -> ! {
    let mut cmd = Cli::command();
    tokimo_bus_cli::print_help_unified(&mut cmd);
    std::process::exit(0);
}
