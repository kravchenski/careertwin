//! Which models to run on this computer, so the program can work offline on an ordinary one.
//!
//! Hearing (Whisper through whisper.cpp) and the English voice (Kokoro) run inside this program.
//! The language model that plays the interviewer is the heavy part, so it runs in a program made
//! for it (Ollama, LM Studio or a llama.cpp server) that this program talks to like any other
//! gateway; this module only says which size fits.

use std::path::{Path, PathBuf};

/// What the computer has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hardware {
    pub ram_mb: u64,
    pub cores: usize,
    pub nvidia: bool,
}

impl Hardware {
    pub fn detect() -> Self {
        Self {
            ram_mb: total_memory_mb().unwrap_or(8 * 1024),
            cores: std::thread::available_parallelism().map_or(4, std::num::NonZero::get),
            nvidia: nvidia_present(),
        }
    }

    pub fn ram_gb(&self) -> u64 {
        self.ram_mb / 1024
    }
}

#[cfg(target_os = "linux")]
fn total_memory_mb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo(&text)
}

#[cfg(not(target_os = "linux"))]
fn total_memory_mb() -> Option<u64> {
    None
}

/// `MemTotal:  16384000 kB` as megabytes.
pub fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes / 1024)
}

fn nvidia_present() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists() || std::path::Path::new("/dev/nvidiactl").exists()
}

/// A Whisper model in the ggml format of whisper.cpp. The hashes are the ones Hugging Face
/// publishes for these files, so a damaged or swapped download is refused.
#[derive(Debug, PartialEq, Eq)]
pub struct SttModel {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    /// Memory the model needs while it runs, in megabytes, with some room to spare.
    pub ram_mb: u64,
}

const WHISPER_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

impl SttModel {
    pub fn url(&self) -> String {
        format!("{WHISPER_BASE_URL}/{}", self.file)
    }

    pub fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(self.file)
    }

    pub fn megabytes(&self) -> u64 {
        self.bytes / 1_000_000
    }
}

/// Smallest to best. All are multilingual, so a slip into Russian is still understood.
pub const STT_MODELS: [SttModel; 4] = [
    SttModel {
        id: "tiny",
        label: "Whisper tiny",
        file: "ggml-tiny-q5_1.bin",
        bytes: 32_152_673,
        sha256: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
        ram_mb: 400,
    },
    SttModel {
        id: "base",
        label: "Whisper base",
        file: "ggml-base-q5_1.bin",
        bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        ram_mb: 500,
    },
    SttModel {
        id: "small",
        label: "Whisper small",
        file: "ggml-small-q5_1.bin",
        bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        ram_mb: 900,
    },
    SttModel {
        id: "turbo",
        label: "Whisper large-v3-turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        ram_mb: 1800,
    },
];

pub fn stt_model(id: &str) -> Option<&'static SttModel> {
    STT_MODELS.iter().find(|m| m.id == id)
}

/// Kokoro-82M, a small multilingual voice with Apache-2.0 weights, pinned to one commit of its
/// Hugging Face repository so the files cannot change under the program.
pub const KOKORO_REVISION: &str = "f3ff3571791e39611d31c381e3a41a3af07b4987";
const KOKORO_URL: &str = "https://huggingface.co/hexgrad/Kokoro-82M/resolve";

/// `(file, bytes, sha256)` of the model itself.
const KOKORO_FILES: [(&str, u64, &str); 2] = [
    ("config.json", 2_351, "5abb01e2403b072bf03d04fde160443e209d7a0dad49a423be15196b9b43c17f"),
    ("kokoro-v1_0.pth", 327_212_226, "496dba118d1a58f5f3db2efc88dbdc216e0483fc89fe6e47ee1f2c53f18ad1e4"),
];

/// English voices: `(name, bytes, sha256, description)`. The first letter of the name is the
/// accent (a American, b British) and the second the voice (f female, m male).
pub const KOKORO_VOICES: [(&str, u64, &str, &str); 12] = [
    ("af_heart", 523_425, "0ab5709b8ffab19bfd849cd11d98f75b60af7733253ad0d67b12382a102cb4ff", "American, warm"),
    ("af_bella", 523_425, "8cb64e02fcc8de0327a8e13817e49c76c945ecf0052ceac97d3081480e8e48d6", "American, bright"),
    ("af_nicole", 523_430, "c5561808bcf5250fe8c5f5de32caf2d94f27e57e95befdb098c5c85991d4c5da", "American, calm"),
    ("af_sarah", 523_425, "49bd364ea3be9eb3e9685e8f9a15448c4883112a7c0ff7ab139fa4088b08cef9", "American, clear"),
    ("af_sky", 523_351, "c799548aed06e0cb0d655a85a01b48e7f10484d71663f9a3045a5b9362e8512c", "American, light"),
    ("am_adam", 523_420, "ced7e284aba12472891be1da3ab34db84cc05cc02b5889535796dbf2d8b0cb34", "American, deep"),
    ("am_michael", 523_435, "9a443b79a4b22489a5b0ab7c651a0bcd1a30bef675c28333f06971abbd47bd37", "American, friendly"),
    ("bf_emma", 523_420, "d0a423deabf4a52b4f49318c51742c54e21bb89bbbe9a12141e7758ddb5da701", "British, polished"),
    ("bf_isabella", 523_440, "cdd4c37003805104d1d08fb1e05855c8fb2c68de24ca6e71f264a30aaa59eefd", "British, soft"),
    ("bm_daniel", 523_430, "fc3fce4e9c12ed4dbc8fa9680cfe51ee190a96444ce7c3ad647549a30823fc5d", "British, steady"),
    ("bm_george", 523_430, "f1bc812213dc59774769e5c80004b13eeb79bd78130b11b2d7f934542dab811b", "British, formal"),
    ("bm_lewis", 523_425, "b5204750dcba01029d2ac9cec17aec3b20a6d64073c579d694a23cb40effbd0e", "British, relaxed"),
];

pub fn kokoro_voice_known(name: &str) -> bool {
    KOKORO_VOICES.iter().any(|(voice, ..)| *voice == name)
}

/// The files Kokoro needs for one voice, to be fetched into one folder.
pub fn kokoro_assets(voice: &str) -> Option<Vec<crate::speech::download::Asset>> {
    use crate::speech::download::Asset;
    let (_, bytes, sha256, _) = KOKORO_VOICES.iter().find(|(name, ..)| *name == voice)?;
    let mut assets: Vec<Asset> = KOKORO_FILES
        .iter()
        .map(|(file, bytes, sha)| Asset::new(format!("{KOKORO_URL}/{KOKORO_REVISION}/{file}"), file, *bytes, sha))
        .collect();
    assets.push(Asset::new(
        format!("{KOKORO_URL}/{KOKORO_REVISION}/voices/{voice}.pt"),
        &format!("voices/{voice}.pt"),
        *bytes,
        sha256,
    ));
    Some(assets)
}

/// Piper, the engine of the offline Russian voice: the release of the project itself, whose archive
/// holds the program, its libraries and the phoneme data.
pub const PIPER_ARCHIVE: &str = "piper_linux_x86_64.tar.gz";
const PIPER_URL: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz";
const PIPER_BYTES: u64 = 26_460_462;
const PIPER_SHA256: &str = "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992";

/// The Russian voices, pinned to one commit of the repository that holds them:
/// `(name, onnx bytes, onnx sha256, config bytes, config sha256, description)`.
pub const PIPER_REVISION: &str = "5041a7a49ae5b3a0f30e6837769eb03fdbb231bd";
const PIPER_VOICE_URL: &str = "https://huggingface.co/rhasspy/piper-voices/resolve";
pub const PIPER_VOICES: [(&str, u64, &str, u64, &str, &str); 2] = [
    (
        "ru_RU-irina-medium",
        63_201_294,
        "8ff38212d23da300bbe3705c645e6e5b9475f0bfde01558eb17813e22acaaaaa",
        4_765,
        "c2ec28bb38e2b59e93b959b3e40348c1afebbd272f30fed5d41205d08e98a9d7",
        "female, even",
    ),
    (
        "ru_RU-denis-medium",
        63_201_294,
        "15fab56e11a097858ee115545d0f697fc2a316c41a291a5362349fb870411b0a",
        4_823,
        "831c860dac0b5073eaa81610a0a638ec23d90a6cf8e5f871b4485c2cec3767c8",
        "male, calm",
    ),
];

/// The Russian voice used when none is chosen.
pub const PIPER_DEFAULT_VOICE: &str = "ru_RU-irina-medium";

pub fn piper_voice_known(name: &str) -> bool {
    PIPER_VOICES.iter().any(|(voice, ..)| *voice == name)
}

/// The voice to use for `name`, which may be a voice of another engine (the settings keep one name
/// for the cloud and one for the offline voice): the default when it is not one of ours.
pub fn piper_voice_or_default(name: &str) -> &str {
    if piper_voice_known(name) { name } else { PIPER_DEFAULT_VOICE }
}

/// Whether this computer can run the offline Russian voice: the engine is built for Linux on x86-64.
pub const PIPER_HERE: bool = cfg!(all(target_os = "linux", target_arch = "x86_64"));

/// The files the Russian voice needs: the engine and one voice.
pub fn piper_assets(voice: &str) -> Option<Vec<crate::speech::download::Asset>> {
    use crate::speech::download::Asset;
    let (name, onnx_bytes, onnx_sha, json_bytes, json_sha, _) = PIPER_VOICES.iter().find(|(n, ..)| *n == voice)?;
    let path = voice.strip_prefix("ru_RU-").and_then(|rest| rest.strip_suffix("-medium"))?;
    let base = format!("{PIPER_VOICE_URL}/{PIPER_REVISION}/ru/ru_RU/{path}/medium/{name}");
    Some(vec![
        Asset::new(PIPER_URL, PIPER_ARCHIVE, PIPER_BYTES, PIPER_SHA256),
        Asset::new(format!("{base}.onnx"), &format!("{name}.onnx"), *onnx_bytes, onnx_sha),
        Asset::new(format!("{base}.onnx.json"), &format!("{name}.onnx.json"), *json_bytes, json_sha),
    ])
}

/// The Whisper model as a download.
pub fn stt_asset(model: &SttModel) -> crate::speech::download::Asset {
    crate::speech::download::Asset::new(model.url(), model.file, model.bytes, model.sha256)
}

/// A model for the interviewer, as the name Ollama pulls it by.
#[derive(Debug, PartialEq)]
pub struct LlmAdvice {
    pub tag: &'static str,
    /// Download size in gigabytes, roughly.
    pub gigabytes: f32,
}

/// Models that follow a spoken conversation and ask real follow-ups need about four billion
/// parameters at the least; below that they lose the thread.
const LLMS: [(u64, LlmAdvice); 4] = [
    (0, LlmAdvice { tag: "gemma3:1b", gigabytes: 0.8 }),
    (6, LlmAdvice { tag: "gemma3:4b", gigabytes: 3.3 }),
    (14, LlmAdvice { tag: "gemma3:12b", gigabytes: 8.1 }),
    (28, LlmAdvice { tag: "gemma3:27b", gigabytes: 17.0 }),
];

/// What to run here.
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub stt: &'static SttModel,
    pub llm: &'static LlmAdvice,
    /// The language model is too small to hold a good interview; the person should know.
    pub llm_is_weak: bool,
}

pub fn recommend(hardware: &Hardware) -> Plan {
    let ram_gb = hardware.ram_gb();
    let stt = match ram_gb {
        0..=3 => &STT_MODELS[0],
        4..=7 => &STT_MODELS[1],
        8..=15 => &STT_MODELS[2],
        _ if hardware.nvidia || hardware.cores >= 8 => &STT_MODELS[3],
        _ => &STT_MODELS[2],
    };
    let llm = LLMS.iter().rev().find(|(from, _)| ram_gb >= *from).map_or(&LLMS[0].1, |(_, advice)| advice);
    Plan { stt, llm, llm_is_weak: ram_gb < 6 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware(ram_gb: u64, cores: usize, nvidia: bool) -> Hardware {
        Hardware { ram_mb: ram_gb * 1024, cores, nvidia }
    }

    #[test]
    fn the_memory_is_read_from_proc_meminfo() {
        let text = "MemTotal:       16384000 kB\nMemFree:         1234 kB\n";
        assert_eq!(parse_meminfo(text), Some(16_000));
        assert_eq!(parse_meminfo("nothing here"), None);
        assert_eq!(parse_meminfo("MemTotal: lots kB"), None);
    }

    #[test]
    fn detection_finds_something_on_any_machine() {
        let hardware = Hardware::detect();
        assert!(hardware.ram_mb >= 256 && hardware.cores >= 1);
    }

    #[test]
    fn a_small_machine_gets_small_models_and_a_warning() {
        let plan = recommend(&hardware(4, 4, false));
        assert_eq!(plan.stt.id, "base");
        assert_eq!(plan.llm.tag, "gemma3:1b");
        assert!(plan.llm_is_weak);
        assert_eq!(recommend(&hardware(2, 2, false)).stt.id, "tiny");
    }

    #[test]
    fn an_ordinary_laptop_gets_whisper_small_and_a_four_billion_model() {
        let plan = recommend(&hardware(8, 8, false));
        assert_eq!(plan.stt.id, "small");
        assert_eq!(plan.llm.tag, "gemma3:4b");
        assert!(!plan.llm_is_weak);
        assert_eq!(
            recommend(&hardware(16, 4, false)).stt.id,
            "small",
            "few cores and no GPU: the big model would be too slow to follow a conversation"
        );
    }

    #[test]
    fn a_strong_machine_gets_the_best_hearing_and_a_bigger_model() {
        assert_eq!(recommend(&hardware(16, 12, false)).stt.id, "turbo");
        assert_eq!(recommend(&hardware(16, 4, true)).stt.id, "turbo");
        assert_eq!(recommend(&hardware(32, 16, true)).llm.tag, "gemma3:27b");
        assert_eq!(recommend(&hardware(16, 8, false)).llm.tag, "gemma3:12b");
    }

    #[test]
    fn the_models_always_fit_in_the_memory_they_are_recommended_for() {
        for ram_gb in [2u64, 3, 4, 6, 8, 12, 16, 24, 32, 64] {
            let plan = recommend(&hardware(ram_gb, 8, ram_gb >= 16));
            let needed_mb = plan.stt.ram_mb + (plan.llm.gigabytes * 1024.0) as u64 + 1500;
            // the small tiers cannot fit everything: they are the ones that carry the warning
            if !plan.llm_is_weak && ram_gb >= 6 {
                assert!(
                    needed_mb <= ram_gb * 1024,
                    "{ram_gb} GB: needs {needed_mb} MB for {} and {}",
                    plan.stt.label,
                    plan.llm.tag
                );
            }
        }
    }

    #[test]
    fn every_model_has_a_real_hash_a_size_and_a_url_on_huggingface() {
        let mut seen = std::collections::HashSet::new();
        for model in &STT_MODELS {
            assert_eq!(model.sha256.len(), 64);
            assert!(model.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(model.bytes > 1_000_000);
            assert!(model.url().starts_with("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-"));
            assert!(model.file.ends_with(".bin") && !model.file.contains('/'));
            assert!(seen.insert(model.id), "{} is listed twice", model.id);
        }
        assert_eq!(stt_model("small").unwrap().file, "ggml-small-q5_1.bin");
        assert!(stt_model("huge").is_none());
        assert_eq!(STT_MODELS[2].megabytes(), 190);
    }

    #[test]
    fn the_voice_files_are_listed_with_real_hashes_and_unique_names() {
        let mut seen = std::collections::HashSet::new();
        for (name, bytes, sha256, description) in KOKORO_VOICES {
            assert!(seen.insert(name), "{name} twice");
            assert_eq!(sha256.len(), 64);
            assert!(bytes > 100_000);
            assert!(!description.is_empty());
            assert!(
                name.starts_with(['a', 'b']) && name.chars().nth(1).is_some_and(|c| c == 'f' || c == 'm'),
                "{name}"
            );
        }
        assert!(kokoro_voice_known("af_heart") && !kokoro_voice_known("../evil"));
    }

    #[test]
    fn a_voice_comes_with_the_model_its_config_and_nothing_else() {
        let assets = kokoro_assets("am_michael").unwrap();
        let names: Vec<&str> = assets.iter().map(|a| a.relative.as_str()).collect();
        assert_eq!(names, ["config.json", "kokoro-v1_0.pth", "voices/am_michael.pt"]);
        assert!(assets.iter().all(|a| a.url.contains(KOKORO_REVISION)
            && a.url.starts_with("https://huggingface.co/hexgrad/Kokoro-82M/resolve/")));
        assert!(kokoro_assets("nobody").is_none());
        assert!(kokoro_assets("../../etc/passwd").is_none());
    }

    #[test]
    fn a_whisper_model_becomes_a_download_with_its_hash() {
        let asset = stt_asset(&STT_MODELS[2]);
        assert_eq!(asset.relative, "ggml-small-q5_1.bin");
        assert_eq!(asset.sha256, STT_MODELS[2].sha256);
        assert_eq!(asset.bytes, STT_MODELS[2].bytes);
    }

    #[test]
    fn the_models_get_better_as_they_grow() {
        let sizes: Vec<u64> = STT_MODELS.iter().map(|m| m.bytes).collect();
        let mut sorted = sizes.clone();
        sorted.sort_unstable();
        assert_eq!(sizes, sorted);
    }
}
