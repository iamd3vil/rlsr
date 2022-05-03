use clap::Parser;
use env_logger::Env;
use log::error;
use releasr::{run, Opts};
use std::process;

use releasr::config::parse_config;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "releasr.yml")]
    config: String,

    #[clap(long, name = "rm-dist")]
    rm_dist: bool,

    #[clap(short, long)]
    publish: bool,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let config = args.config;

    let cfg = parse_config(&config).await;
    let cfg = match cfg {
        Ok(cfg) => cfg,
        Err(err) => {
            error!("error parsing config: {}", err);
            process::exit(1);
        }
    };

    let opts = Opts {
        publish: args.publish,
        rm_dist: args.rm_dist,
    };

    if let Err(error) = run(cfg, opts).await {
        error!("error running releasr: {}", error);
        process::exit(1);
    }
}
