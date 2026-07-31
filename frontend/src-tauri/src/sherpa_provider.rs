use anyhow::{anyhow, Result};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig};

pub(crate) const SHERPA_PROVIDER_OVERRIDE_ENV: &str = "MEETILY_SHERPA_ONNX_PROVIDER";
pub(crate) const SHERPA_THREADS_OVERRIDE_ENV: &str = "MEETILY_SHERPA_ONNX_THREADS";
pub(crate) const SHERPA_DEBUG_ENV: &str = "MEETILY_SHERPA_ONNX_DEBUG";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SherpaModelFamily {
    SenseVoice,
    Qwen3Asr,
}

impl SherpaModelFamily {
    fn display_name(self) -> &'static str {
        match self {
            Self::SenseVoice => "SenseVoice",
            Self::Qwen3Asr => "Qwen3-ASR",
        }
    }

    fn supports(self, provider: SherpaProvider) -> bool {
        match self {
            Self::SenseVoice => true,
            Self::Qwen3Asr => provider != SherpaProvider::CoreMl,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SherpaProvider {
    Cpu,
    Cuda,
    CoreMl,
}

impl SherpaProvider {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cpu" => Some(Self::Cpu),
            "cuda" => Some(Self::Cuda),
            "coreml" => Some(Self::CoreMl),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            Self::CoreMl => "coreml",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetPlatform {
    MacOs,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProviderSelection {
    requested: SherpaProvider,
    ignored_override: Option<String>,
}

pub(crate) fn create_offline_recognizer(
    family: SherpaModelFamily,
    base_config: &OfflineRecognizerConfig,
) -> Result<OfflineRecognizer> {
    let override_value = std::env::var(SHERPA_PROVIDER_OVERRIDE_ENV).ok();
    let selection = select_provider(
        family,
        current_platform(),
        cfg!(feature = "sherpa-cuda"),
        override_value.as_deref(),
    );

    if let Some(value) = selection.ignored_override.as_deref() {
        log::warn!(
            "Ignoring {}='{}' for {} because it is invalid or unsupported; using '{}'",
            SHERPA_PROVIDER_OVERRIDE_ENV,
            value,
            family.display_name(),
            selection.requested.as_str()
        );
    }

    let num_threads = configured_num_threads(base_config.model_config.num_threads);
    log::info!(
        "Configuring {} sherpa-onnx recognizer with {} inference thread{}",
        family.display_name(),
        num_threads,
        if num_threads == 1 { "" } else { "s" }
    );
    create_with_fallback(family, selection.requested, |provider| {
        let mut config = base_config.clone();
        config.model_config.provider = Some(provider.as_str().to_string());
        config.model_config.num_threads = num_threads;
        config.model_config.debug = env_flag(SHERPA_DEBUG_ENV);
        OfflineRecognizer::create(&config)
    })
}

fn configured_num_threads(default: i32) -> i32 {
    let value = std::env::var(SHERPA_THREADS_OVERRIDE_ENV).ok();
    parse_num_threads(value.as_deref(), default)
}

fn parse_num_threads(value: Option<&str>, default: i32) -> i32 {
    let Some(value) = value else {
        return default;
    };
    match value.trim().parse::<i32>() {
        Ok(threads) if (1..=8).contains(&threads) => threads,
        _ => {
            log::warn!(
                "Ignoring {}='{}'; expected an integer from 1 to 8",
                SHERPA_THREADS_OVERRIDE_ENV,
                value
            );
            default
        }
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn current_platform() -> TargetPlatform {
    if cfg!(target_os = "macos") {
        TargetPlatform::MacOs
    } else {
        TargetPlatform::Other
    }
}

fn select_provider(
    family: SherpaModelFamily,
    platform: TargetPlatform,
    sherpa_cuda_native: bool,
    override_value: Option<&str>,
) -> ProviderSelection {
    let default = default_provider(family, platform, sherpa_cuda_native);
    let Some(value) = override_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ProviderSelection {
            requested: default,
            ignored_override: None,
        };
    };

    if value.eq_ignore_ascii_case("auto") {
        return ProviderSelection {
            requested: default,
            ignored_override: None,
        };
    }

    match SherpaProvider::parse(value).filter(|provider| {
        family.supports(*provider) && provider_is_available(*provider, platform, sherpa_cuda_native)
    }) {
        Some(provider) => ProviderSelection {
            requested: provider,
            ignored_override: None,
        },
        None => ProviderSelection {
            requested: default,
            ignored_override: Some(value.to_string()),
        },
    }
}

fn provider_is_available(
    provider: SherpaProvider,
    platform: TargetPlatform,
    sherpa_cuda_native: bool,
) -> bool {
    match provider {
        SherpaProvider::Cpu => true,
        SherpaProvider::CoreMl => platform == TargetPlatform::MacOs,
        SherpaProvider::Cuda => platform != TargetPlatform::MacOs && sherpa_cuda_native,
    }
}

fn default_provider(
    _family: SherpaModelFamily,
    platform: TargetPlatform,
    sherpa_cuda_native: bool,
) -> SherpaProvider {
    if platform != TargetPlatform::MacOs && sherpa_cuda_native {
        return SherpaProvider::Cuda;
    }

    SherpaProvider::Cpu
}

fn create_with_fallback<T, F>(
    family: SherpaModelFamily,
    requested: SherpaProvider,
    mut create: F,
) -> Result<T>
where
    F: FnMut(SherpaProvider) -> Option<T>,
{
    log::info!(
        "Creating {} sherpa-onnx recognizer (requested provider='{}')",
        family.display_name(),
        requested.as_str()
    );

    if let Some(recognizer) = create(requested) {
        log::info!(
            "Created {} sherpa-onnx recognizer (requested provider='{}', configured provider='{}', app fallback='none'); sherpa-onnx does not expose the ONNX Runtime provider actually used per operation",
            family.display_name(),
            requested.as_str(),
            requested.as_str()
        );
        return Ok(recognizer);
    }

    if requested == SherpaProvider::Cpu {
        return Err(anyhow!(
            "sherpa-onnx could not create the {} recognizer with requested provider='cpu'",
            family.display_name()
        ));
    }

    log::warn!(
        "sherpa-onnx could not create the {} recognizer with requested provider='{}'; falling back to provider='cpu'",
        family.display_name(),
        requested.as_str()
    );
    if let Some(recognizer) = create(SherpaProvider::Cpu) {
        log::info!(
            "Created {} sherpa-onnx recognizer (requested provider='{}', configured provider='cpu', app fallback='cpu')",
            family.display_name(),
            requested.as_str()
        );
        return Ok(recognizer);
    }

    Err(anyhow!(
        "sherpa-onnx could not create the {} recognizer with requested provider='{}' or CPU fallback",
        family.display_name(),
        requested.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sense_voice_auto_uses_the_benchmarked_cpu_path_on_macos() {
        let selection = select_provider(
            SherpaModelFamily::SenseVoice,
            TargetPlatform::MacOs,
            false,
            None,
        );

        assert_eq!(selection.requested, SherpaProvider::Cpu);
    }

    #[test]
    fn qwen_stays_on_cpu_on_macos() {
        let selection = select_provider(
            SherpaModelFamily::Qwen3Asr,
            TargetPlatform::MacOs,
            false,
            None,
        );

        assert_eq!(selection.requested, SherpaProvider::Cpu);
    }

    #[test]
    fn generic_native_build_does_not_enable_sherpa_cuda() {
        let selection = select_provider(
            SherpaModelFamily::SenseVoice,
            TargetPlatform::Other,
            false,
            None,
        );

        assert_eq!(selection.requested, SherpaProvider::Cpu);
    }

    #[test]
    fn dedicated_native_cuda_build_enables_cuda_off_macos() {
        for family in [SherpaModelFamily::SenseVoice, SherpaModelFamily::Qwen3Asr] {
            let selection = select_provider(family, TargetPlatform::Other, true, None);
            assert_eq!(selection.requested, SherpaProvider::Cuda);
        }
    }

    #[test]
    fn diagnostic_override_respects_model_provider_support() {
        let sense_voice = select_provider(
            SherpaModelFamily::SenseVoice,
            TargetPlatform::MacOs,
            false,
            Some("cpu"),
        );
        assert_eq!(sense_voice.requested, SherpaProvider::Cpu);
        assert_eq!(sense_voice.ignored_override, None);

        let sense_voice_coreml = select_provider(
            SherpaModelFamily::SenseVoice,
            TargetPlatform::MacOs,
            false,
            Some("coreml"),
        );
        assert_eq!(sense_voice_coreml.requested, SherpaProvider::CoreMl);
        assert_eq!(sense_voice_coreml.ignored_override, None);

        let qwen = select_provider(
            SherpaModelFamily::Qwen3Asr,
            TargetPlatform::MacOs,
            false,
            Some("coreml"),
        );
        assert_eq!(qwen.requested, SherpaProvider::Cpu);
        assert_eq!(qwen.ignored_override.as_deref(), Some("coreml"));

        let unavailable_cuda = select_provider(
            SherpaModelFamily::SenseVoice,
            TargetPlatform::Other,
            false,
            Some("cuda"),
        );
        assert_eq!(unavailable_cuda.requested, SherpaProvider::Cpu);
        assert_eq!(unavailable_cuda.ignored_override.as_deref(), Some("cuda"));

        let packaged_cuda = select_provider(
            SherpaModelFamily::SenseVoice,
            TargetPlatform::Other,
            true,
            Some("cuda"),
        );
        assert_eq!(packaged_cuda.requested, SherpaProvider::Cuda);
        assert_eq!(packaged_cuda.ignored_override, None);
    }

    #[test]
    fn failed_accelerated_creation_retries_cpu() {
        let mut attempts = Vec::new();
        let result = create_with_fallback(
            SherpaModelFamily::SenseVoice,
            SherpaProvider::CoreMl,
            |provider| {
                attempts.push(provider);
                (provider == SherpaProvider::Cpu).then_some("recognizer")
            },
        )
        .unwrap();

        assert_eq!(result, "recognizer");
        assert_eq!(attempts, vec![SherpaProvider::CoreMl, SherpaProvider::Cpu]);
    }

    #[test]
    fn successful_accelerated_creation_does_not_retry_cpu() {
        let mut attempts = Vec::new();
        let result = create_with_fallback(
            SherpaModelFamily::SenseVoice,
            SherpaProvider::CoreMl,
            |provider| {
                attempts.push(provider);
                Some("recognizer")
            },
        )
        .unwrap();

        assert_eq!(result, "recognizer");
        assert_eq!(attempts, vec![SherpaProvider::CoreMl]);
    }

    #[test]
    fn thread_override_is_bounded() {
        assert_eq!(parse_num_threads(Some("2"), 3), 2);
        assert_eq!(parse_num_threads(Some("12"), 3), 3);
        assert_eq!(parse_num_threads(Some("invalid"), 3), 3);
        assert_eq!(parse_num_threads(None, 3), 3);
    }
}
