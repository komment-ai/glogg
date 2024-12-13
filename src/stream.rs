use tokio::io::{AsyncBufRead, AsyncBufReadExt};
use tokio_stream::{wrappers::LinesStream, StreamExt};

use crate::log::Log;

pub async fn transpose<F>(
    // TODO: perhaps could be an AsycnStream, iirc tokio exposes stdout as an async stream
    mut writer: impl std::io::Write,
    reader: impl AsyncBufRead + Unpin,
) -> std::io::Result<()>
where
    Log<F>: std::fmt::Display,
{
    let mut lines = LinesStream::new(reader.lines());
    let mut yaml_buffer = String::new();

    while let Some(line) = lines.next().await {
        let line = line.unwrap();
        if line == "---" {
            // Parse the collected YAML block if it's non-empty
            if !yaml_buffer.trim().is_empty() {
                match Log::<F>::parse(&yaml_buffer) {
                    Some(entry) => {
                        write!(writer, "{entry}")?;
                    }
                    None => {
                        eprintln!("Failed to parse log entry:\n{}", yaml_buffer);
                    }
                }
                yaml_buffer.clear();
            }
        } else {
            // Add line to buffer
            let line = line.replace("\t", "    ");
            yaml_buffer.push_str(&line);
            yaml_buffer.push('\n');
        }
    }

    Ok(writer.flush()?)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use tokio::io::BufReader;

    use crate::{log::Raw, stream::transpose};

    // Mock for `process_stream_output` using a simulated log stream
    #[tokio::test]
    async fn test_transpose() {
        // TODO: this isn't exactly the format that gcloud sends.
        // TODO: this is a perfect use case for a fuzzer
        let log_data = concat!(
            "---\n",
            "json_payload:\n",
            "  message: \"Test message\"\n",
            "  time: \"2023-01-01T12:00:00Z\"\n",
            "resource:\n",
            "  labels:\n",
            "    instance_id: \"test-instance\"\n",
            "---\n"
        );
        let reader = BufReader::new(Cursor::new(log_data));
        let mut result = Vec::new();

        transpose::<Raw>(&mut result, reader).await.unwrap();
        let result = String::from_utf8(result).unwrap();
        assert_eq!(result, "Test message");
    }
}
