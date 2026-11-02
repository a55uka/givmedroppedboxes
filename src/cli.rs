use std::path::PathBuf;

use clap::Parser;

/// Browse a Dropbox share link and download the files and folders you pick.
#[derive(Parser, Debug)]
#[command(name = "givmedroppedboxes", version, about)]
pub struct Args {
    /// Dropbox share link to open (skips the paste screen)
    pub link: Option<String>,

    /// Directory to download into (default: ./<share name>)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Number of parallel downloads
    #[arg(short = 'j', long, default_value_t = 3)]
    pub jobs: usize,
}
