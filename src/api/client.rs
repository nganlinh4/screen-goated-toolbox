use lazy_static::lazy_static;

lazy_static! {
    pub static ref UREQ_AGENT: ureq::Agent = ureq::AgentBuilder::new()
        .timeout_read(std::time::Duration::from_secs(30))
        .timeout_write(std::time::Duration::from_secs(30))
        .build();
}
