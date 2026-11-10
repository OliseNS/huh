use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};

const HUH_AUDIO: &[u8] = include_bytes!("../assets/huh.mp3");

const SOUND_SCRIPT: &str = "\
file=$1
if command -v paplay >/dev/null 2>&1; then
    paplay \"$file\"
elif command -v pw-play >/dev/null 2>&1; then
    pw-play \"$file\"
elif command -v ffplay >/dev/null 2>&1; then
    ffplay -nodisp -autoexit -loglevel quiet \"$file\"
elif command -v mpv >/dev/null 2>&1; then
    mpv --no-video --really-quiet \"$file\"
elif command -v gst-play-1.0 >/dev/null 2>&1; then
    gst-play-1.0 -q \"$file\"
elif command -v cvlc >/dev/null 2>&1; then
    cvlc --play-and-exit --intf dummy \"$file\"
elif command -v afplay >/dev/null 2>&1; then
    afplay \"$file\"
fi
rm -f \"$file\"
";

pub(crate) fn should_play(no_sound: bool, config_enabled: bool) -> bool {
    !no_sound && config_enabled
}

pub(crate) fn play() -> Option<JoinHandle<()>> {
    if !io::stdout().is_terminal() {
        return None;
    }

    Some(thread::spawn(|| {
        let _ = play_file();
    }))
}

fn play_file() -> Result<(), String> {
    let path = write_sound_file()?;
    let spawned = Command::new("sh")
        .arg("-c")
        .arg(SOUND_SCRIPT)
        .arg("sh")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(error) = spawned {
        let _ = fs::remove_file(&path);
        return Err(format!("failed to play sound: {error}"));
    }
    Ok(())
}

fn write_sound_file() -> Result<PathBuf, String> {
    let dir = env::temp_dir();
    for attempt in 0..10 {
        let path = dir.join(format!("huh-{}-{attempt}.mp3", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(HUH_AUDIO)
                    .map_err(|error| format!("failed to write sound: {error}"))?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("failed to write sound: {error}")),
        }
    }
    Err("failed to create sound file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_an_mp3_file() {
        assert!(HUH_AUDIO.len() > 1000);
        assert_eq!(HUH_AUDIO[0], 0xFF);
        assert_eq!(HUH_AUDIO[1] & 0xE0, 0xE0);
    }

    #[test]
    fn sound_can_be_disabled() {
        assert!(should_play(false, true));
        assert!(!should_play(true, true));
        assert!(!should_play(false, false));
    }
}
