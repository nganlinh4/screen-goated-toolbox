use lazy_static::lazy_static;
use std::time::Duration;

lazy_static! {
    pub static ref UREQ_AGENT: ureq::Agent = {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        config.into()
    };
}
