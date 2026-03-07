use std::path::PathBuf;

use clap::Parser;

use crate::types::Browser;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(long)]
    pub xqh: Option<String>,

    #[arg(long, short)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub class_times: Option<PathBuf>,

    #[arg(long)]
    pub input_json: Option<PathBuf>,

    #[arg(long)]
    pub url: Option<String>,

    #[arg(long, value_enum)]
    pub browser: Option<Browser>,
}
