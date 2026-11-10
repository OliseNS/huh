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
    let mut chunk = [0u8; 16 * 1024];
    let mut stdin = io::stdin().lock();

    loop {
        let read = stdin
            .read(&mut chunk)
            .map_err(|error| format!("failed to read stdin: {error}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > STDIN_LIMIT_BYTES * 2 {
            let excess = bytes.len() - STDIN_LIMIT_BYTES;
            bytes.drain(..excess);
        }
    }

    let (text, truncated) = tail_lossy(&bytes, STDIN_LIMIT_BYTES);
    if truncated && !text.is_empty() {
        eprintln!("warning: stdin truncated to the last {STDIN_LIMIT_BYTES} bytes");
        return Ok(format!(
            "[input truncated: showing the last {STDIN_LIMIT_BYTES} bytes]\n{text}"
        ));
    }
    Ok(text)
}

fn tail_lossy(bytes: &[u8], limit: usize) -> (String, bool) {
    let truncated = bytes.len() > limit;
    let mut start = bytes.len().saturating_sub(limit);
    while start < bytes.len() && bytes[start] & 0b1100_0000 == 0b1000_0000 {
        start += 1;
    }
    (
        String::from_utf8_lossy(&bytes[start..]).trim().to_string(),
        truncated,
    )
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

    #[test]
    fn keeps_the_tail_of_oversized_input() {
        let mut input = b"old\n".to_vec();
        input.extend_from_slice(&[b'x'; 32]);
        input.extend_from_slice(b"\nlast error");

        let (text, truncated) = tail_lossy(&input, 16);
        assert!(truncated);
        assert!(text.ends_with("last error"));
        assert!(!text.contains("old"));

        let (text, truncated) = tail_lossy(b"small", 16);
        assert!(!truncated);
        assert_eq!(text, "small");
    }
}
