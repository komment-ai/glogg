use anyhow::Result;
use std::{process::Stdio, str};
use tokio::process::{self, Child};

pub trait GcloudCommand {
    // I may recall something about a command trait.. could be useful here
    fn new() -> process::Command;
}

pub struct Gcloud;
impl GcloudCommand for Gcloud {
    fn new() -> process::Command {
        process::Command::new("gcloud")
    }
}

pub fn is_authed() -> bool {
    // @simon this is wrong, it does not fail when not authenticated
    std::process::Command::new("gcloud")
        .args(["projects", "list"])
        .stdout(Stdio::piped())
        .spawn()
        .is_ok()
}

pub async fn get_current_project<G: GcloudCommand>() -> Result<String> {
    Ok(str::from_utf8(
        &G::new()
            .args(["config", "get", "project"])
            .output()
            .await?
            .stdout,
    )?
    .trim()
    .to_string())
}

pub async fn fetch_instance_filters<G: GcloudCommand>(
    project: &str,
    filter: &str,
) -> Result<String> {
    Ok(str::from_utf8(
        &G::new()
            .args([
                "compute",
                "instances",
                "list",
                "--filter",
                filter,
                "--format",
                "get(id)",
                "--project",
                project,
            ])
            .output()
            .await?
            .stdout,
    )?
    .split("\n")
    .filter(|id| !id.is_empty())
    .map(|id| format!("resource.labels.instance_id={id}"))
    .collect::<Vec<_>>()
    .join(" OR "))
}

pub async fn start_log_stream<G: GcloudCommand>(project: &str, filter: &str) -> Result<Child> {
    Ok(G::new()
        .args(["alpha", "logging", "tail", filter, "--project", project])
        .stdout(Stdio::piped())
        .spawn()?)
}

pub async fn get_log_slice<G: GcloudCommand>(project: &str, filter: &str) -> Result<Child> {
    Ok(G::new()
        .args([
            "logging",
            "read",
            filter,
            "--project",
            project,
            "--order",
            "asc",
        ])
        .stdout(Stdio::piped())
        .spawn()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockGcloud;
    impl GcloudCommand for MockGcloud {
        fn new() -> process::Command {
            process::Command::new("echo")
        }
    }

    #[tokio::test]
    async fn test_fetch_instance_filters() {
        let filter = "test-filter";
        let result = fetch_instance_filters::<MockGcloud>("mock-project", filter)
            .await
            .unwrap();

        assert!(result.contains("resource.labels.instance_id="));
    }

    #[tokio::test]
    async fn test_get_log_slice() {
        let filter = "test-log-filter";
        let output = get_log_slice::<MockGcloud>("mock-project", filter)
            .await
            .unwrap();

        assert!(
            str::from_utf8(&output.wait_with_output().await.unwrap().stdout)
                .unwrap()
                .contains("logging read test-log-filter")
        );
    }
}
