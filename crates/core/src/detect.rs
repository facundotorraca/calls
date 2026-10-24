//! Decides when a call starts and ends from what apps are doing with audio.
//!
//! Pure logic: it is fed snapshots of audio activity (once a second, by the audio sidecar)
//! and the current time, so it can be tested without any audio device.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// An app that makes calls, matched by bundle id prefix so its helper processes count too
/// (e.g. `com.google.Chrome` matches `com.google.Chrome.helper`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallApp {
    pub bundle_prefix: String,
    pub name: String,
}

impl CallApp {
    fn new(bundle_prefix: &str, name: &str) -> CallApp {
        CallApp { bundle_prefix: bundle_prefix.into(), name: name.into() }
    }

    fn owns(&self, process: &AudioProcess) -> bool {
        process.bundle_id.starts_with(&self.bundle_prefix)
    }

    pub fn defaults() -> Vec<CallApp> {
        vec![
            CallApp::new("us.zoom.xos", "Zoom"),
            CallApp::new("com.microsoft.teams", "Teams"),
            CallApp::new("com.google.Chrome", "Chrome"),
            CallApp::new("company.thebrowser", "Arc"),
            CallApp::new("com.brave.Browser", "Brave"),
            CallApp::new("com.microsoft.edgemac", "Edge"),
            CallApp::new("org.mozilla.firefox", "Firefox"),
            CallApp::new("com.apple.WebKit.GPU", "Safari"),
            CallApp::new("com.apple.FaceTime", "FaceTime"),
            CallApp::new("com.apple.avconferenced", "FaceTime"),
            CallApp::new("com.tinyspeck.slackmacgap", "Slack"),
            CallApp::new("com.hnc.Discord", "Discord"),
            CallApp::new("net.whatsapp.WhatsApp", "WhatsApp"),
            CallApp::new("Cisco-Systems.Spark", "Webex"),
        ]
    }
}

/// A process using audio, as reported by the audio sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioProcess {
    pub pid: i32,
    pub bundle_id: String,
    /// Capturing audio (microphone).
    pub input: bool,
    /// Playing audio.
    pub output: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Detection {
    Started(CallApp),
    Ended(CallApp),
}

/// A call starts when a call app holds the microphone for a moment, and ends when that app
/// stops using audio altogether. Output counts too: muting yourself in a call releases the
/// microphone in some apps, but the other side keeps playing.
pub struct CallDetector {
    apps: Vec<CallApp>,
    start_after: Duration,
    end_after: Duration,
    candidate: Option<(CallApp, Instant)>,
    active: Option<(CallApp, Option<Instant>)>,
}

impl CallDetector {
    pub fn new(apps: Vec<CallApp>) -> CallDetector {
        CallDetector::with_timing(apps, Duration::from_secs(2), Duration::from_secs(10))
    }

    pub fn with_timing(apps: Vec<CallApp>, start_after: Duration, end_after: Duration) -> CallDetector {
        CallDetector { apps, start_after, end_after, candidate: None, active: None }
    }

    pub fn update(&mut self, processes: &[AudioProcess], now: Instant) -> Option<Detection> {
        if let Some((app, idle_since)) = &mut self.active {
            if processes.iter().any(|p| app.owns(p) && (p.input || p.output)) {
                *idle_since = None;
            } else if idle_since.is_none() {
                *idle_since = Some(now);
            } else if idle_since.is_some_and(|t| now.duration_since(t) >= self.end_after) {
                let (app, _) = self.active.take().expect("active");
                return Some(Detection::Ended(app));
            }
            return None;
        }

        let using_mic = self.apps.iter().find(|app| processes.iter().any(|p| app.owns(p) && p.input));
        match (using_mic, &self.candidate) {
            (None, _) => self.candidate = None,
            (Some(app), Some((candidate, since))) if candidate == app => {
                if now.duration_since(*since) >= self.start_after {
                    self.candidate = None;
                    self.active = Some((app.clone(), None));
                    return Some(Detection::Started(app.clone()));
                }
            }
            (Some(app), _) => self.candidate = Some((app.clone(), now)),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom(input: bool, output: bool) -> AudioProcess {
        AudioProcess { pid: 1, bundle_id: "us.zoom.xos".into(), input, output }
    }

    fn siri() -> AudioProcess {
        AudioProcess { pid: 2, bundle_id: "com.apple.CoreSpeech".into(), input: true, output: false }
    }

    fn at(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    #[test]
    fn detects_a_call_and_its_end() {
        let t = Instant::now();
        let mut detector = CallDetector::new(CallApp::defaults());

        assert_eq!(detector.update(&[zoom(true, true)], at(t, 0)), None);
        assert_eq!(detector.update(&[zoom(true, true)], at(t, 1)), None);
        assert!(
            matches!(detector.update(&[zoom(true, true)], at(t, 2)), Some(Detection::Started(app)) if app.name == "Zoom")
        );

        // Muted: mic released but the other side is still playing.
        assert_eq!(detector.update(&[zoom(false, true)], at(t, 30)), None);
        assert_eq!(detector.update(&[zoom(false, true)], at(t, 60)), None);

        // Hung up.
        assert_eq!(detector.update(&[], at(t, 61)), None);
        assert_eq!(detector.update(&[], at(t, 65)), None);
        assert!(matches!(detector.update(&[], at(t, 71)), Some(Detection::Ended(_))));
    }

    #[test]
    fn ignores_brief_mic_use_and_non_call_apps() {
        let t = Instant::now();
        let mut detector = CallDetector::new(CallApp::defaults());

        assert_eq!(detector.update(&[zoom(true, false)], at(t, 0)), None);
        assert_eq!(detector.update(&[], at(t, 1)), None);
        assert_eq!(detector.update(&[zoom(true, false)], at(t, 2)), None);

        for s in 10..20 {
            assert_eq!(detector.update(&[siri()], at(t, s)), None);
        }
    }

    #[test]
    fn helper_processes_count_as_the_app() {
        let t = Instant::now();
        let mut detector = CallDetector::new(CallApp::defaults());
        let meet = AudioProcess { pid: 3, bundle_id: "com.google.Chrome.helper".into(), input: true, output: true };

        detector.update(std::slice::from_ref(&meet), at(t, 0));
        assert!(matches!(detector.update(&[meet], at(t, 3)), Some(Detection::Started(app)) if app.name == "Chrome"));
    }
}
