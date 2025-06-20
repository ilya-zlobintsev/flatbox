use clap::{Parser, Subcommand};

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
    pub command: String,
    pub args: Vec<String>,
}
