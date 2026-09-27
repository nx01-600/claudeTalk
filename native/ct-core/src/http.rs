//! The one HTTP agent every download uses (the dictation app from GitHub,
//! the models from Hugging Face).

use std::time::Duration;

/// ureq 3 defaults to Rustls, which isn't compiled in: an agent built without
/// picking NativeTls panics on its first https request.
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build())
        .build()
        .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_builds_with_native_tls() {
        let a = super::agent();
        assert_eq!(a.config().tls_config().provider(), ureq::tls::TlsProvider::NativeTls);
    }

    /// Real https request; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn https_request_works() {
        let resp = super::agent()
            .head("https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin")
            .call()
            .unwrap();
        assert!(resp.status().is_success());
    }
}
