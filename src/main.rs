use core::str;
use std::process::{exit, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};
use tokio_stream::{wrappers::LinesStream, StreamExt};

use clap::Parser;
use inline_colorization::*;
use serde::Deserialize;

// TODO: AppSettings::AllowLeadingHyphen

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    filter: String,
    #[clap(short, long)]
    pretty: bool,
    #[clap(short, long, requires_all=["start", "stop"])]
    get: bool,
    #[clap(long, allow_hyphen_values = true)]
    start: Option<String>,
    #[clap(long, allow_hyphen_values = true)]
    stop: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Payload {
    message: String,
    time: String,
}
#[derive(Deserialize, Debug)]
struct Labels {
    instance_id: String,
}
#[derive(Deserialize, Debug)]
struct Resource {
    labels: Labels,
}
#[derive(Deserialize, Debug)]
struct Log {
    #[serde(alias = "jsonPayload")]
    json_payload: Payload,
    resource: Resource,
}
#[tokio::main]
async fn main() {
    std::env::set_var("CLOUDSDK_PYTHON_SITEPACKAGES", "1");
    let Args {
        filter,
        pretty,
        get,
        start,
        stop,
    } = Args::parse();

    let mut gcloud = Command::new("gcloud");

    let mut filters = fetch_instance_filters(&mut gcloud, &filter).await;

    if get {
        let Ok(start_time) = parse_datetime::parse_datetime(start.unwrap()) else {
            exit(0);
        };
        let Ok(stop_time) = parse_datetime::parse_datetime(stop.unwrap()) else {
            exit(0);
        };
        // For testing
        let filters = format!(
            "({}) AND timestamp<=\"{}\" AND timestamp>=\"{}\"",
            filters,
            stop_time.format("%+"),
            start_time.format("%+")
        );
        print!("{filters}");
        let mut gcloud = Command::new("gcloud");
        let output = get_log_slice(&mut gcloud, &filters).await;
        let log_lines = output.lines().map(|s| Ok(s.to_string()));
        process_slice_output(log_lines, pretty);
        exit(0);
    }
    let mut gcloud = Command::new("gcloud");
    let mut stream = start_log_stream(&mut gcloud, &filters).await;

    let stdout = stream.stdout.take().expect("Failed to capture stdout");
    let reader = BufReader::new(stdout);
    let lines = LinesStream::new(reader.lines());

    let mut current_task = tokio::spawn(process_stream_output(lines, pretty));

    loop {
        sleep(Duration::from_secs(30)).await;

        // Fetch the latest instance filters
        let mut gcloud = Command::new("gcloud");
        let new_filters = fetch_instance_filters(&mut gcloud, &filter).await;

        // If the instance list has changed, update the filters and restart the stream
        if new_filters != filters {
            filters = new_filters;

            // Kill the old stream
            stream.kill().await.expect("Failed to kill old log stream");

            current_task.abort();

            // Start a new stream with the updated filters
            let mut gcloud = Command::new("gcloud");
            stream = start_log_stream(&mut gcloud, &filters).await;

            // Process the new stream output
            let stdout = stream.stdout.take().expect("Failed to capture stdout");
            let reader = BufReader::new(stdout);
            let lines = LinesStream::new(reader.lines());
            current_task = tokio::spawn(process_stream_output(lines, pretty));
        }
    }
}

// Helper function to fetch instance filters based on the filter parameter
async fn fetch_instance_filters(command: &mut Command, filter: &str) -> String {
    let instances = command
        .args([
            "compute",
            "instances",
            "list",
            "--filter",
            filter,
            "--format",
            "get(id)",
        ])
        .output()
        .await
        .expect("Failed to execute gcloud command");

    str::from_utf8(&instances.stdout)
        .unwrap()
        .split("\n")
        .filter(|id| !id.is_empty())
        .map(|id| format!("resource.labels.instance_id={id}"))
        .collect::<Vec<_>>()
        .join(" OR ")
}

async fn start_log_stream(command: &mut Command, filter: &str) -> Child {
    command
        .args(["alpha", "logging", "tail", filter])
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start gcloud logging tail")
}

async fn get_log_slice(command: &mut Command, filter: &str) -> String {
    let output = command
        .args(["logging", "read", filter])
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute gcloud read command")
        .wait_with_output()
        .await
        .expect("Failed to read output from gcloud command");

    String::from_utf8(output.stdout).expect("Failed to convert output to String")
}

async fn process_stream_output(mut lines: LinesStream<impl AsyncBufReadExt + Unpin>, pretty: bool) {
    let mut yaml_buffer = String::new();

    while let Some(line) = lines.next().await {
        let line = line.unwrap();
        if line == "---" {
            // Parse the collected YAML block if it's non-empty
            if !yaml_buffer.trim().is_empty() {
                print_log_entry(&yaml_buffer, pretty);
                yaml_buffer.clear();
            }
        } else {
            // Add line to buffer
            let line = line.replace("\t", "    ");
            yaml_buffer.push_str(&line);
            yaml_buffer.push('\n');
        }
    }
}

fn process_slice_output<I>(lines: I, pretty: bool)
where
    I: Iterator<Item = Result<String, std::io::Error>>,
{
    let mut yaml_buffer = String::new();

    for line in lines {
        let line = line.unwrap();
        if line == "---" {
            // Parse the collected YAML block if it's non-empty
            if !yaml_buffer.trim().is_empty() {
                print_log_entry(&yaml_buffer, pretty);
                yaml_buffer.clear();
            }
        } else {
            // Add line to buffer
            let line = line.replace("\t", "    ");
            yaml_buffer.push_str(&line);
            yaml_buffer.push('\n');
        }
    }

    // Handle any remaining YAML in the buffer after the loop
    if !yaml_buffer.trim().is_empty() {
        print_log_entry(&yaml_buffer, pretty);
    }
}

fn print_log_entry(yaml: &str, pretty: bool) {
    let Ok(Log {
        json_payload: Payload { message, time },
        resource: Resource {
            labels: Labels { instance_id },
        },
    }) = serde_yaml::from_str(yaml)
    else {
        return;
    };
    if pretty {
        let format_time = format!("{: <30}", time);
        let format_id = format!("{: <19}", instance_id);
        print!("[{color_yellow}{format_id}{color_reset}] {color_green}{format_time}{color_reset} | {message}");
    } else {
        print!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::BufReader;
    use tokio_stream::wrappers::LinesStream;

    // Test `fetch_instance_filters` by simulating `gcloud` output
    #[tokio::test]
    async fn test_fetch_instance_filters() {
        // Replace this with actual test data or mocking logic if using a mocking crate
        let mut mock_gcloud = Command::new("echo");
        let filter = "test-filter";
        let result = fetch_instance_filters(&mut mock_gcloud, filter).await;

        // Assert that the result is in the expected format, e.g., "resource.labels.instance_id=..."
        assert!(result.contains("resource.labels.instance_id="));
    }

    // Test `get_log_slice` to confirm it correctly reads from `gcloud` output (using a mock approach)
    #[tokio::test]
    async fn test_get_log_slice() {
        let mut mock_gcloud = Command::new("echo");
        let filter = "test-log-filter";
        let output = get_log_slice(&mut mock_gcloud, filter).await;

        // Check if the output contains expected content (assuming mock gcloud returns test data)
        assert!(output.contains("logging read test-log-filter"));
    }

    // Test `process_slice_output` by passing it YAML-formatted log data
    #[test]
    fn test_process_slice_output() {
        let log_data = r#"
            ---
            json_payload:
              message: "Test message"
              time: "2023-01-01T12:00:00Z"
            resource:
              labels:
                instance_id: "test-instance"
            ---
        "#;

        // Simulate lines of YAML logs
        let lines = log_data.lines().map(|line| Ok(line.to_string()));
        process_slice_output(lines, true);

        // You would check for expected print output if you captured it using `print!` or `println!`
    }

    // Test `print_log_entry` directly by passing sample YAML data
    #[test]
    fn test_print_log_entry() {
        let yaml_data = r#"
            json_payload:
              message: "Test message"
              time: "2023-01-01T12:00:00Z"
            resource:
              labels:
                instance_id: "test-instance"
        "#;

        print_log_entry(yaml_data, true);
        // Validate printed output if capturing stdout (not shown here for simplicity)
    }

    // Mock for `process_stream_output` using a simulated log stream
    #[tokio::test]
    async fn test_process_stream_output() {
        let log_data = r#"
            ---
            json_payload:
              message: "Test message"
              time: "2023-01-01T12:00:00Z"
            resource:
              labels:
                instance_id: "test-instance"
            ---
        "#;
        let reader = BufReader::new(Cursor::new(log_data));
        let lines = LinesStream::new(reader.lines());

        process_stream_output(lines, true).await;
        // Validate printed output if capturing stdout (not shown here for simplicity)
    }
}
