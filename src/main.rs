use clap::Parser;
use releasr::{parse_config, run};
use std::process;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "releasr.yml")]
    config: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let config = args.config;

    let cfg = parse_config(&config).await;
    let cfg = match cfg {
        Ok(cfg) => cfg,
        Err(err) => {
            println!("error parsing config: {}", err);
            process::exit(1);
        }
    };

    if let Err(error) = run(cfg).await {
        println!("error running releasr: {}", error);
        process::exit(1);
    }
}
