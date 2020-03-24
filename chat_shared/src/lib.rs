use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub targets: Vec<String>,
    pub message: String,
}

impl Message {
    pub fn easy_parse(input: &str) -> Option<Message> {
        let (dest, msg) = input
            .find(':')
            .map(|idx| (&input[..idx], input[idx + 1..].trim()))?;

        let targets: Vec<String> = dest
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();

        let message: String = msg.to_string();

        Some(Message { targets, message })
    }
}
