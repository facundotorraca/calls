use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use calls_core::engines::{AudioExtractor, AudioInfo, MixInput, Progress};

use crate::process::{Stream, run_lines};
use crate::sidecar;

#[derive(Clone)]
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

    /// Runs ffmpeg, reporting progress against the expected output duration.
    fn run(&self, mut cmd: Command, duration_ms: Option<u64>, progress: Progress) -> Result<()> {
        cmd.args(["-progress", "pipe:1", "-nostats", "-stats_period", "0.5"]);
        run_lines(cmd, None, Stream::Stdout, |line| {
            // `-progress` prints key=value pairs; out_time_us is the position reached so far.
            if let (Some(total), Some(us)) = (duration_ms, line.strip_prefix("out_time_us="))
                && let Ok(us) = us.parse::<u64>()
            {
                progress(us as f32 / 1000.0 / total.max(1) as f32);
            }
        })
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-nostdin", "-y", "-hide_banner", "-loglevel", "error"]);
        cmd
    }
}

impl AudioExtractor for Ffmpeg {
    fn extract(&self, input: &Path, output: &Path, progress: Progress) -> Result<AudioInfo> {
        let duration_ms = self.duration_ms(input);
        let mut cmd = self.command();
        cmd.arg("-i").arg(input).args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"]).arg(output);
        self.run(cmd, duration_ms, progress)?;
        Ok(AudioInfo { duration_ms })
    }

    fn mix(&self, inputs: &[MixInput], output: &Path, progress: Progress) -> Result<()> {
        let duration_ms = inputs.iter().filter_map(|i| Some(self.duration_ms(i.file)? + i.offset_ms)).max();
        let mut cmd = self.command();
        for input in inputs {
            cmd.arg("-i").arg(input.file);
        }
        cmd.args(["-filter_complex", &mix_filter(inputs)])
            .args(["-map", "[out]", "-vn", "-ac", "1", "-c:a", "aac", "-b:a", "64k"])
            // Index up front, so the player can seek before reading the whole file.
            .args(["-movflags", "+faststart"])
            .arg(output);
        self.run(cmd, duration_ms, progress)
    }
}

/// Delays each input by its offset, then sums them. Summed rather than averaged (which would
/// make a two track call half as loud), with a limiter against clipping.
fn mix_filter(inputs: &[MixInput]) -> String {
    let mut filter = String::new();
    for (i, input) in inputs.iter().enumerate() {
        filter.push_str(&format!("[{i}:a:0]adelay={}:all=1[a{i}];", input.offset_ms));
    }
    for i in 0..inputs.len() {
        filter.push_str(&format!("[a{i}]"));
    }
    filter.push_str(&format!("amix=inputs={}:duration=longest:normalize=0,alimiter=limit=0.95[out]", inputs.len()));
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_tracks_at_their_offsets() {
        let (mic, system) = (Path::new("mic.m4a"), Path::new("system.m4a"));
        let filter = mix_filter(&[MixInput { file: mic, offset_ms: 0 }, MixInput { file: system, offset_ms: 1200 }]);
        assert_eq!(
            filter,
            "[0:a:0]adelay=0:all=1[a0];[1:a:0]adelay=1200:all=1[a1];\
             [a0][a1]amix=inputs=2:duration=longest:normalize=0,alimiter=limit=0.95[out]"
        );
    }
}
