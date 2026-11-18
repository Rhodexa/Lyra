//! Project files (`.lyra`): readable JSON with a format tag and a version.
//!
//! The version only goes up when a change can't be read by older code. New
//! fields don't need it: missing fields fall back to their defaults, so old
//! files keep opening.

use crate::Project;
use serde::{Deserialize, Serialize};

pub const EXTENSION: &str = "lyra";
const FORMAT: &str = "lyra-project";
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    format: String,
    version: u32,
    project: Project,
}

pub fn to_json(project: &Project) -> String {
    let file = ProjectFile { format: FORMAT.into(), version: VERSION, project: project.clone() };
    serde_json::to_string_pretty(&file).expect("a Project always serializes")
}

pub fn from_json(text: &str) -> Result<Project, String> {
    let file: ProjectFile = serde_json::from_str(text).map_err(|e| format!("not a readable Lyra project ({e})"))?;
    if file.format != FORMAT {
        return Err(format!("not a Lyra project (format \"{}\")", file.format));
    }
    if file.version > VERSION {
        return Err(format!("made by a newer Lyra (file v{}, this Lyra reads up to v{VERSION})", file.version));
    }
    Ok(file.project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, Note, TICKS_PER_BAR};

    #[test]
    fn round_trip_keeps_everything() {
        let mut p = Project::default();
        let ch = p.add_channel("Bass", "lyra.synth", vec![0.0, 1.0, 0.5]);
        p.patterns[0].notes_mut(ch).push(Note { start: 48, len: 36, key: 45, velocity: 0.9 });
        p.add_pattern();
        let id = p.patterns[0].id;
        p.clips.push(Clip::of_pattern(id, 2, TICKS_PER_BAR, 4 * TICKS_PER_BAR));
        let a = p.add_audio("Hello", "/tmp/hello.wav", 1.5);
        p.audio[a].fade_out = crate::Fade { len: 0.25, tension: -0.5 };
        let mut cl = Clip::of_audio(p.audio[a].id, 3, 0, 100);
        cl.offset = 12;
        cl.gain = -3.5;
        cl.fade_in = crate::Fade { len: 0.1, tension: 0.3 };
        p.clips.push(cl);
        p.tracks[3].mixer = 4;
        p.tracks[2].mute = true;
        p.bpm = 128.5;
        let back = from_json(&to_json(&p)).unwrap();
        assert_eq!(back, p);
        // ids keep counting from where they were (no reuse after loading)
        let mut back = back;
        assert_ne!(back.add_channel("x", "lyra.kick", vec![]), ch);
    }

    #[test]
    fn clips_from_before_audio_still_open() {
        let json = r#"{"format":"lyra-project","version":1,"project":{"clips":[{"pattern":7,"track":1,"start":0,"len":384}]}}"#;
        let p = from_json(json).unwrap();
        assert_eq!(p.clips[0].pattern(), Some(7));
        assert_eq!(p.clips[0].offset, 0);
        // And pattern clips are still written the old way.
        assert!(to_json(&p).contains("\"pattern\": 7"));
    }

    #[test]
    fn refuses_files_from_the_future_and_strangers() {
        let mut json: serde_json::Value = serde_json::from_str(&to_json(&Project::default())).unwrap();
        json["version"] = (VERSION + 1).into();
        assert!(from_json(&json.to_string()).unwrap_err().contains("newer Lyra"));
        json["format"] = "something-else".into();
        assert!(from_json(&json.to_string()).is_err());
        assert!(from_json("{ not json").is_err());
    }

    #[test]
    fn older_files_missing_fields_still_open() {
        let mut json: serde_json::Value = serde_json::from_str(&to_json(&Project::default())).unwrap();
        json["project"].as_object_mut().unwrap().remove("clips");
        json["project"].as_object_mut().unwrap().remove("tracks");
        let p = from_json(&json.to_string()).unwrap();
        assert!(p.clips.is_empty());
        assert_eq!(p.tracks.len(), crate::DEFAULT_TRACKS);
    }
}
