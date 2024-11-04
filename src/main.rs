use std::{process::exit, str};

use anyhow::anyhow;
use atty::Stream;
use clap::Parser;
use tokio::{
    io::BufReader,
    time::{sleep, Duration},
};

use gcloud::*;
use log::{Decorated, Log, Pretty, Raw};

mod gcloud;
mod log;
mod stream;

/// View Google Cloud Plaform VM Instance logs based on a label attached to the service.
///
/// Glögg is a simple command-line tool for streaming or fetching Google Cloud logs from VM
/// Compute instances based on a filter.
///
/// Features:
///
///   - Streaming: tail logs from instances by specifying a filter rather than individual instance IDs
///
///   - Fetch: Use the same filters to retrieve all logs between a start and stop time
///
///   - Real-Time Parsing: Stream logs directly from Google Cloud Logging, parse them as YAML, and highlight key information for quick readability.
#[derive(Parser, Debug)]
#[clap(author, about, version)]
struct Args {
    /// Filter logs based on a label attached to the service
    #[clap(short, long, global = true, default_value = "")]
    filter: String,

    /// Pretty print the logs including the instance ID and timestamp
    #[clap(short, long, global = true)]
    pretty: bool,

    /// The Google Cloud project to use
    #[clap(long, global = true)]
    project: Option<String>,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    /// Fetch logs in a given time range
    Get {
        /// The start of the time range to fetch logs in
        #[clap(short = 'i', long, allow_hyphen_values = true)]
        start: String,

        /// The end of the time range to fetch logs in
        #[clap(short = 'j', long, allow_hyphen_values = true, default_value = "now")]
        stop: String,
    },

    /// Stream logs in real-time
    Watch,
}

impl Command {
    async fn run<F>(self, project: &str, filter: &str) -> anyhow::Result<()>
    where
        Log<F>: std::fmt::Display + 'static,
    {
        let mut filters = fetch_instance_filters::<Gcloud>(&project, &filter).await?;

        Ok(match self {
            // TODO: this is missing historical instance IDs
            Command::Get { start, stop } => {
                let start = parse_datetime::parse_datetime(start)?;
                let stop = parse_datetime::parse_datetime(stop)?;
                let filters = format!(
                    "({filters}) AND timestamp<=\"{}\" AND timestamp>=\"{}\"",
                    stop.format("%+"),
                    start.format("%+")
                );

                stream::transpose::<F>(
                    std::io::stdout(),
                    BufReader::new(
                        get_log_slice::<Gcloud>(project, &filters)
                            .await?
                            .stdout
                            .take()
                            .ok_or(anyhow!("Failed to capture stdout"))?,
                    ),
                )
                .await?
            }
            Command::Watch => {
                let mut stream;
                let mut current_task;

                loop {
                    stream = start_log_stream::<Gcloud>(project, &filters).await?;
                    current_task = tokio::spawn(stream::transpose::<F>(
                        std::io::stdout(),
                        BufReader::new(
                            stream
                                .stdout
                                .take()
                                .ok_or(anyhow!("Failed to capture stdout"))?,
                        ),
                    ));

                    let mut new_filters;
                    while {
                        new_filters = fetch_instance_filters::<Gcloud>(project, filter).await?;
                        new_filters == filters
                    } {
                        sleep(Duration::from_secs(30)).await;
                    }
                    filters = new_filters;

                    stream.kill().await.expect("Failed to kill old log stream");
                    current_task.abort();
                }
            }
        })
    }
}

#[tokio::main]
async fn main() {
    std::env::set_var("CLOUDSDK_PYTHON_SITEPACKAGES", "1");
    let Args {
        filter,
        pretty,
        project,
        command,
    } = Args::parse();

    // Check if we're logged into gcloud
    if !gcloud::is_authed() {
        eprintln!("Run `gcloud auth login` to authenticate");
        exit(1);
    }

    let project = match project {
        Some(proj) => proj,
        None => match get_current_project::<gcloud::Gcloud>().await {
            Ok(proj) => proj,
            Err(_) => {
                eprintln!("Failed to get current project");
                exit(1);
            }
        },
    };

    if let Err(e) = match pretty {
        true => match atty::is(Stream::Stdout) {
            true => command.run::<Decorated>(&project, &filter).await,
            false => command.run::<Pretty>(&project, &filter).await,
        },
        false => command.run::<Raw>(&project, &filter).await,
    } {
        eprintln!("Error: {}", e);
        exit(1);
    }
}
