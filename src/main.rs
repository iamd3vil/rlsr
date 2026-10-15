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

    /// Build without a release tag, for testing. Uncommitted changes are
    /// allowed, the version comes from snapshot.version_template, and
    /// nothing is published.
    #[clap(long)]
    snapshot: bool,

    /// Build HEAD and publish it as the nightly, replacing the previous one
    /// under the rolling nightly tag. The version comes from
    /// nightly.version_template.
    #[clap(long, conflicts_with = "snapshot")]
    nightly: bool,

    /// Skip each build's command, prehook and posthook; package and publish
    /// existing artifacts. Release hooks still run.
    #[clap(long)]
    skip_build: bool,

    /// Build and package only, and write a manifest to the dist folder so a
    /// later --merge run can checksum and publish. For building targets on
    /// separate CI runners.
    #[clap(long)]
    split: bool,

    /// With --split, only run builds whose target is in this comma-separated
    /// list.
    #[clap(long, use_value_delimiter = true, requires = "split")]
    targets: Vec<String>,

    /// Skip hooks and builds; checksum and publish the archives listed in the
    /// split manifests in the dist folder.
    #[clap(long, conflicts_with_all = &["split", "skip-build", "rm-dist"])]
    merge: bool,
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
        snapshot: args.snapshot,
        nightly: args.nightly,
        rm_dist: args.rm_dist,
        skip_build: args.skip_build,
        split: args.split,
        merge: args.merge,
        targets: args.targets,
    };

    rlsr::run(cfg, opts).await
}
