use std::process;
use clap::Parser;
use env_logger::Env;
use releasr::{parse_config, run};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "releasr.yml")]
    config: String,
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let config = args.config;

    let cfg = parse_config(&config);
    let cfg = match cfg {
        Ok(cfg) => cfg,
        Err(err) => {
            println!("error parsing config: {}", err);
            process::exit(1);
        }
    };

    if let Err(error) = run(&cfg) {
        println!("error running releasr: {}", error);
        process::exit(1);
    }
}
