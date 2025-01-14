use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Log<Format> {
    #[serde(alias = "jsonPayload")]
    json_payload: Payload,
    resource: Resource,

    #[serde(skip)]
    _fmt: std::marker::PhantomData<Format>,
}

impl<Format> Log<Format> {
    pub fn parse(yaml: &str, include_failures: bool) -> Option<Self> {
        let cleaned = strip_ansi_escapes::strip_str(yaml);

        match serde_yaml::from_str(&cleaned) {
            Ok(log) => Some(log),
            Err(e) => {
                if include_failures {
                    eprintln!("Failed to parse YAML: {}", e);
                    eprintln!("YAML content:\n{}", cleaned);
                }
                None
            }
        }
    }
}

/// A log formatter sentinel type that renders colorful and detailed logs.
pub struct Decorated;

impl std::fmt::Display for Log<Decorated> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use inline_colorization::*;
        let Log {
            json_payload: Payload { message, time },
            resource: Resource {
                labels: Labels { instance_id },
            },
            ..
        } = self;

        // Define an array of acceptable colors
        let colors = [color_yellow, color_green, color_blue, color_red, color_cyan];

        // Determine the color based on instance_id
        let color_index = instance_id.chars().map(|c| c as usize).sum::<usize>() % colors.len();
        let selected_color = colors[color_index];

        write!(
            f,
            "[{selected_color}{instance_id: <19}{color_reset}] {color_green}{time: <30}{color_reset} | {message}"
        )
    }
}

/// A log formatter sentinel type that renders detailed logs.
pub struct Pretty;

impl std::fmt::Display for Log<Pretty> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Log {
            json_payload: Payload { message, time },
            resource: Resource {
                labels: Labels { instance_id },
            },
            ..
        } = self;
        write!(f, "[{instance_id: <19}] {time: <30} | {message}",)
    }
}

/// A log formatter sentinel type that renders log messages only.
pub struct Raw;

impl std::fmt::Display for Log<Raw> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Log {
            json_payload: Payload { message, .. },
            ..
        } = self;

        write!(f, "{message}")
    }
}

#[derive(Deserialize, Debug)]
pub struct Resource {
    labels: Labels,
}

#[derive(Deserialize, Debug)]
pub struct Labels {
    instance_id: String,
}

#[derive(Deserialize, Debug)]
pub struct Payload {
    message: String,
    time: String,
}

#[cfg(test)]
mod tests {
    use inline_colorization::*;

    use super::{Decorated, Log, Pretty, Raw};

    macro_rules! test {
        ($name:ident: $format:ty => $expected_output:expr) => {
            #[test]
            fn $name() {
                let yaml_data = r#"
                    json_payload:
                      message: "Test message"
                      time: "2023-01-01T12:00:00Z"
                    resource:
                      labels:
                        instance_id: "test-instance"
                "#;

                let expected_output = format!($expected_output);
                assert_eq!(
                    Log::<$format>::parse(yaml_data, true).unwrap().to_string(),
                    expected_output
                );
            }
        };
    }

    test!(
        test_decorated:
        Decorated =>
        "[{color_green}test-instance      {color_reset}] {color_green}2023-01-01T12:00:00Z          {color_reset} | Test message"
    );

    test!(
        test_pretty:
        Pretty =>
        "[test-instance      ] 2023-01-01T12:00:00Z           | Test message"
    );

    test!(
        test_raw:
        Raw =>
        "Test message"
    );
}
