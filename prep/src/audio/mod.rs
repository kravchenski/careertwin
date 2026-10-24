//! Sound in and out: the microphone, the speakers, and the cutting of speech into turns.

pub mod capture;
pub mod playback;
pub mod resample;
pub mod vad;

pub use capture::Mic;
pub use playback::{Player, Speaker};

/// The names worth showing in a list of devices. The sound system also lists plugins and sinks
/// that are not something you speak into or listen to.
pub fn friendly_devices(names: Vec<String>) -> Vec<String> {
    const NOT_DEVICES: [&str; 6] =
        ["Plugin", "Discard all samples", "Rate Converter", "Open Sound System", "JACK Audio", "plugin"];
    let mut kept: Vec<String> = Vec::new();
    for name in names {
        if NOT_DEVICES.iter().any(|bad| name.contains(bad)) || kept.contains(&name) {
            continue;
        }
        kept.push(name);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::friendly_devices;

    #[test]
    fn plugins_and_duplicates_are_left_out_of_the_lists() {
        let raw = [
            "Discard all samples (playback) or generate zero samples (capture)",
            "Rate Converter Plugin Using Libav/FFmpeg Library",
            "JACK Audio Connection Kit",
            "Open Sound System",
            "PipeWire Sound Server",
            "Plugin for channel upmix (4,6,8)",
            "HD-Audio Generic, ALC245 Analog",
            "HD-Audio Generic, ALC245 Analog",
            "Default ALSA Output (currently PipeWire Media Server)",
        ];
        let kept = friendly_devices(raw.iter().map(|s| (*s).to_owned()).collect());
        assert_eq!(
            kept,
            vec![
                "PipeWire Sound Server",
                "HD-Audio Generic, ALC245 Analog",
                "Default ALSA Output (currently PipeWire Media Server)"
            ]
        );
    }
}
