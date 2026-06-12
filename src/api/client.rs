use lazy_static::lazy_static;
use std::time::Duration;

lazy_static! {
    pub static ref UREQ_AGENT: ureq::Agent = {
        let config = ureq::Agent::config_builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        config.into()
    };
}
