//! Where the answers come from: a Switchyard gateway or any provider with an OpenAI-style API.
//!
//! All of them take `POST <base>/v1/chat/completions` and `GET <base>/v1/models`, so a provider is just a base
//! address (plus a key for the cloud ones).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub url: &'static str,
    /// Needs an API key, and receives your CV and ABOUT text with every request.
    pub cloud: bool,
}

pub const PROVIDERS: [Provider; 8] = [
    Provider { id: "switchyard", label: "Switchyard (local)", url: "http://127.0.0.1:3260", cloud: false },
    Provider { id: "groq", label: "Groq", url: "https://api.groq.com/openai", cloud: true },
    Provider { id: "openai", label: "OpenAI", url: "https://api.openai.com", cloud: true },
    Provider { id: "openrouter", label: "OpenRouter", url: "https://openrouter.ai/api", cloud: true },
    Provider { id: "nvidia", label: "NVIDIA", url: "https://integrate.api.nvidia.com", cloud: true },
    Provider { id: "tokenharbor", label: "TokenHarbor", url: "https://tokenharbor.ai", cloud: true },
    Provider { id: "ollama", label: "Ollama (local)", url: "http://localhost:11434", cloud: false },
    Provider { id: "lmstudio", label: "LM Studio (local)", url: "http://localhost:1234", cloud: false },
];

pub const OTHER: &str = "other";

/// The address without a trailing slash, `/v1` or `/chat/completions`: the form people paste.
pub fn base_url(url: &str) -> String {
    let mut url = url.trim().trim_end_matches('/');
    for suffix in ["/v1/chat/completions", "/chat/completions", "/v1"] {
        if let Some(stripped) = url.strip_suffix(suffix) {
            url = stripped.trim_end_matches('/');
            break;
        }
    }
    url.to_owned()
}

/// Where Switchyard listens, unless SWITCHYARD_URL says otherwise.
pub fn default_gateway() -> String {
    base_url(&std::env::var("SWITCHYARD_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".to_owned()))
}

/// The preset this address belongs to, if any.
pub fn provider_for(url: &str) -> Option<Provider> {
    let wanted = base_url(url).to_lowercase();
    PROVIDERS.iter().copied().find(|provider| provider.url.to_lowercase() == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pasted_forms_of_an_address_are_one_address() {
        for text in [
            "https://api.groq.com/openai",
            "https://api.groq.com/openai/",
            "https://api.groq.com/openai/v1",
            "https://api.groq.com/openai/v1/chat/completions",
            "  https://api.groq.com/openai/chat/completions ",
        ] {
            assert_eq!(base_url(text), "https://api.groq.com/openai", "{text}");
        }
    }

    #[test]
    fn a_preset_is_found_by_its_address_however_it_was_written() {
        assert_eq!(provider_for("HTTPS://API.GROQ.COM/openai/v1/").map(|p| p.id), Some("groq"));
        assert_eq!(provider_for("http://localhost:11434").map(|p| p.cloud), Some(false));
        assert!(provider_for("https://llm.example.com").is_none());
    }

    #[test]
    fn nvidia_and_tokenharbor_are_cloud_presets_found_from_what_people_paste() {
        let nvidia = provider_for("https://integrate.api.nvidia.com/v1/").expect("NVIDIA is a preset");
        let harbor = provider_for("HTTPS://tokenharbor.ai/v1/chat/completions").expect("TokenHarbor is a preset");
        assert_eq!((nvidia.id, nvidia.label, nvidia.cloud), ("nvidia", "NVIDIA", true));
        assert_eq!((harbor.id, harbor.label, harbor.cloud), ("tokenharbor", "TokenHarbor", true));
    }

    #[test]
    fn no_two_presets_share_an_id_or_an_address() {
        for (i, a) in PROVIDERS.iter().enumerate() {
            for b in &PROVIDERS[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.url.to_lowercase(), b.url.to_lowercase());
            }
        }
    }
}
