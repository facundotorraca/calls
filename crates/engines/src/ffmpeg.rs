use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use calls_core::engines::{AudioExtractor, AudioInfo, Progress};

use crate::process::{Stream, run_lines};
use crate::sidecar;

pub struct Ffmpeg {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

impl Ffmpeg {
    pub fn locate() -> Result<Ffmpeg> {
        Ok(Ffmpeg {
            ffmpeg: sidecar::locate("ffmpeg").context("ffmpeg not found (brew install ffmpeg)")?,
            ffprobe: sidecar::locate("ffprobe").context("ffprobe not found (brew install ffmpeg)")?,
        })
    }

    fn duration_ms(&self, input: &Path) -> Option<u64> {
        let out = Command::new(&self.ffprobe)
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
            .arg(input)
            .output()
            .ok()?;
        let seconds: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some((seconds * 1000.0) as u64)
    }
}

impl AudioExtractor for Ffmpeg {
    fn extract(&self, input: &Path, output: &Path, progress: Progress) -> Result<AudioInfo> {
        let duration_ms = self.duration_ms(input);

        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-nostdin", "-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(input)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
            .args(["-progress", "pipe:1", "-nostats", "-stats_period", "0.5"])
            .arg(output);

        run_lines(cmd, None, Stream::Stdout, |line| {
            // `-progress` prints key=value pairs; out_time_us is the position reached so far.
            if let (Some(total), Some(us)) = (duration_ms, line.strip_prefix("out_time_us="))
                && let Ok(us) = us.parse::<u64>()
            {
                progress(us as f32 / 1000.0 / total.max(1) as f32);
            }
        })?;
        Ok(AudioInfo { duration_ms })
    }
}
