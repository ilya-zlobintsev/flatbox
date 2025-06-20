use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Run(RunCommand),
}

#[derive(Parser)]
pub struct RunCommand {
    /// Flatpak app id (com.example.example) to use as the environment.
    #[arg(long)]
    pub app: Option<String>,
    /// Flatpak runtime id in its full format (org.gnome.Platform/x86_64/48) to use as the environment. Mutually exclusive with `--app`.
    #[arg(long)]
    pub runtime: Option<String>,
    /// Additional Flatpak installation dirs (/var/lib/flatpak and $HOME/.local/share/flatpak are used by default)
    #[arg(long)]
    pub flatpak_install_path: Vec<PathBuf>,
    pub command: String,
    pub args: Vec<String>,
}
