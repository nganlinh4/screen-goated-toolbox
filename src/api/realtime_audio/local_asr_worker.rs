#[path = "local_asr_worker/client.rs"]
mod client;
#[path = "local_asr_worker/model_lease.rs"]
mod model_lease;
#[path = "local_asr_worker/process.rs"]
mod process;

pub(crate) use client::LocalAsrClient;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use model_lease::{
    ModelKind, ModelRemovalOutcome, current_notice as model_notice,
    request_remove as request_model_remove,
};
pub(crate) use sgt_local_asr_protocol::Mode as LocalAsrMode;
#[cfg(feature = "recorder-worker")]
pub(crate) use sgt_local_asr_protocol::TimedToken;
