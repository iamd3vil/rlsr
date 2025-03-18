use clap::Parser;
use color_eyre::{eyre::bail, Result};
use env_logger::Env;
use rlsr::Opts;

use rlsr::config::parse_config;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "rlsr.yml")]
    config: String,

    #[clap(long, name = "rm-dist")]
    rm_dist: bool,

    #[clap(short, long)]
    skip_publish: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().unwrap();
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let config = args.config;

    let cfg = parse_config(&config);
    let cfg = match cfg {
        Ok(cfg) => cfg,
        Err(err) => {
            bail!("error parsing config: {}", err);
        }
    };

    let opts = Opts {
        skip_publish: args.skip_publish,
        rm_dist: args.rm_dist,
    };

    rlsr::run(cfg, opts).await
}
