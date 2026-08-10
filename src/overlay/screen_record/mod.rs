#[cfg(feature = "recorder-worker")]
include!("worker.rs");

#[cfg(not(feature = "recorder-worker"))]
include!("host.rs");
