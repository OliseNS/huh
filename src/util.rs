use std::env;
use std::io::{self, IsTerminal, Read};

const STDIN_LIMIT_BYTES: usize = 256 * 1024;
const SECRET_LIMIT_BYTES: u64 = 4096;

pub(crate) fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn env_value(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .and_then(|value| non_empty(&value).map(str::to_owned))
}

pub(crate) fn read_prompt_stdin() -> Result<String, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(STDIN_LIMIT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read stdin: {error}"))?;

    if bytes.len() <= STDIN_LIMIT_BYTES {
        return Ok(String::from_utf8_lossy(&bytes).trim().to_string());
    }

    let mut start = bytes.len() - STDIN_LIMIT_BYTES;
    while start < bytes.len() && bytes[start] & 0b1100_0000 == 0b1000_0000 {
        start += 1;
    }

    eprintln!("warning: stdin truncated to the last {STDIN_LIMIT_BYTES} bytes");
    Ok(format!(
        "[input truncated: showing the last {STDIN_LIMIT_BYTES} bytes]\n{}",
        String::from_utf8_lossy(&bytes[start..]).trim()
    ))
}

pub(crate) fn read_secret(label: &str) -> Result<String, String> {
    if io::stdin().is_terminal() {
        rpassword::prompt_password(label)
            .map_err(|error| format!("failed to read API key: {error}"))
    } else {
        let mut buffer = String::new();
        io::stdin()
            .take(SECRET_LIMIT_BYTES)
            .read_to_string(&mut buffer)
            .map_err(|error| format!("failed to read API key: {error}"))?;
        Ok(buffer.lines().next().unwrap_or_default().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_blank_values() {
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty("  value \n"), Some("value"));
    }
}
