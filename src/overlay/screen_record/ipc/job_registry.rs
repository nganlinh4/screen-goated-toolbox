//! Shared scaffolding for the long-running IPC job registries (subtitle
//! generation/translation, the narration jobs, and Gemini Translate narration).
//!
//! Only the genuinely-identical plumbing lives here: the per-job handle, the
//! `OnceLock<Mutex<HashMap>>` accessor, the "find an active job" scan, and the
//! job-id generator. Each handler keeps its own snapshot type, results-revision
//! delta rules, cancel side effects, and `run_job` logic because those diverge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Per-job handle shared across every registry: the live snapshot plus a
/// cancellation flag. Generic over the handler-specific snapshot type.
#[derive(Clone)]
pub(super) struct JobHandle<S> {
    pub(super) snapshot: Arc<Mutex<S>>,
    pub(super) cancelled: Arc<AtomicBool>,
    /// Creation order, so eviction drops the oldest job. Job ids carry no
    /// usable order: client ids are random and ticket sequences are decimal.
    created: u64,
}

/// Snapshot types that expose their lifecycle `state` so the active-job scan can
/// be shared. Every job snapshot already stores `state: String`.
pub(super) trait JobState {
    fn state(&self) -> &str;
}

impl<S> JobHandle<S> {
    pub(super) fn new(snapshot: Arc<Mutex<S>>, cancelled: Arc<AtomicBool>) -> Self {
        static CREATED: AtomicU64 = AtomicU64::new(0);
        Self {
            snapshot,
            cancelled,
            created: CREATED.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl<S: JobState> JobHandle<S> {
    /// Whether this job still owns the single active slot. Cancel publishes a
    /// terminal state at once, but the job thread (which owns a clone of
    /// `cancelled`) may still be finishing a provider call; until it drops that
    /// clone the job keeps blocking new starts and is never evicted.
    fn is_active(&self) -> bool {
        let thread_alive = Arc::strong_count(&self.cancelled) > 1;
        if self.cancelled.load(Ordering::SeqCst) && thread_alive {
            return true;
        }
        self.snapshot
            .lock()
            .is_ok_and(|snapshot| matches!(snapshot.state(), "queued" | "running"))
    }
}

/// `get_or_init` accessor for a registry's static `OnceLock<Mutex<HashMap>>`.
pub(super) fn registry<S>(
    cell: &'static OnceLock<Mutex<HashMap<String, JobHandle<S>>>>,
) -> &'static Mutex<HashMap<String, JobHandle<S>>> {
    cell.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the id of the first active job (queued, running, or cancelled but
/// still winding down), if any. Shared by every single-active-job registry.
pub(super) fn find_active<S: JobState>(jobs: &HashMap<String, JobHandle<S>>) -> Option<String> {
    jobs.iter()
        .find_map(|(job_id, handle)| handle.is_active().then(|| job_id.clone()))
}

const MAX_RETAINED_JOBS: usize = 64;

/// Evicts the oldest terminal snapshots before a new job is inserted. Active
/// jobs are never evicted, and a fully-active registry fails closed.
pub(super) fn prepare_for_insert<S: JobState>(
    jobs: &mut HashMap<String, JobHandle<S>>,
) -> Result<(), String> {
    if jobs.len() < MAX_RETAINED_JOBS {
        return Ok(());
    }
    let mut terminal = jobs
        .iter()
        .filter(|(_, handle)| !handle.is_active())
        .map(|(job_id, handle)| (handle.created, job_id.clone()))
        .collect::<Vec<_>>();
    terminal.sort_unstable();
    let remove_count = jobs
        .len()
        .saturating_sub(MAX_RETAINED_JOBS.saturating_sub(1));
    for (_, job_id) in terminal.into_iter().take(remove_count) {
        jobs.remove(&job_id);
    }
    if jobs.len() >= MAX_RETAINED_JOBS {
        return Err(format!(
            "Job registry is full with {MAX_RETAINED_JOBS} active or locked jobs"
        ));
    }
    Ok(())
}

pub(super) fn cancel_all<S>(jobs: &HashMap<String, JobHandle<S>>) {
    for handle in jobs.values() {
        handle.cancelled.store(true, Ordering::SeqCst);
    }
}

const MAX_CLIENT_REQUESTS: usize = 8192;
static CLIENT_REQUESTS: OnceLock<Result<Mutex<ClientRequests>, String>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NarrationKind {
    Subtitle,
    S2s,
    GeminiTranslate,
    Transcription,
}

impl NarrationKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "subtitle" => Some(Self::Subtitle),
            "s2s" => Some(Self::S2s),
            "gemini-translate" => Some(Self::GeminiTranslate),
            "transcription" => Some(Self::Transcription),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Subtitle => "subtitle",
            Self::S2s => "s2s",
            Self::GeminiTranslate => "gemini-translate",
            Self::Transcription => "transcription",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Subtitle => 0,
            Self::S2s => 1,
            Self::GeminiTranslate => 2,
            Self::Transcription => 3,
        }
    }
}

#[derive(Default)]
struct TicketState {
    issued: u64,
    watermark: u64,
}

pub(super) struct ClientRequests {
    legacy: HashMap<String, ClientRequest>,
    epoch: String,
    tickets: [TicketState; 4],
}

impl ClientRequests {
    fn new() -> Result<Self, String> {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| "Narration identity is unavailable".to_string())?;
        let epoch = random.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self {
            legacy: HashMap::new(),
            epoch,
            tickets: Default::default(),
        })
    }

    fn allocate(&mut self, kind: NarrationKind) -> Result<String, String> {
        let state = &mut self.tickets[kind.index()];
        state.issued = state
            .issued
            .checked_add(1)
            .ok_or("Narration ticket sequence exhausted")?;
        Ok(format!(
            "ticket-{}-{}-{}",
            self.epoch,
            kind.label(),
            state.issued
        ))
    }

    fn retire_ticket(
        &mut self,
        job_id: &str,
        expected: NarrationKind,
        allow_stale: bool,
    ) -> Result<bool, String> {
        let Some(ticket) = job_id.strip_prefix("ticket-") else {
            return Ok(false);
        };
        let (epoch, rest) = ticket.split_once('-').ok_or("Invalid narration ticket")?;
        let (kind, sequence) = rest.rsplit_once('-').ok_or("Invalid narration ticket")?;
        let kind = NarrationKind::parse(kind).ok_or("Invalid narration ticket kind")?;
        let sequence: u64 = sequence
            .parse()
            .map_err(|_| "Invalid narration ticket sequence")?;
        if kind != expected || sequence == 0 {
            return Err("Invalid narration ticket".to_string());
        }
        if epoch != self.epoch {
            if allow_stale {
                return Ok(true);
            }
            return Err("Narration ticket belongs to another worker".to_string());
        }
        let state = &mut self.tickets[kind.index()];
        if sequence > state.issued {
            return Err("Unknown narration ticket".to_string());
        }
        if sequence <= state.watermark {
            if allow_stale {
                return Ok(true);
            }
            return Err("Narration ticket already retired or completed".to_string());
        }
        state.watermark = sequence;
        Ok(true)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ClientRequest {
    Started,
    Retired,
}

pub(super) fn client_requests() -> Result<std::sync::MutexGuard<'static, ClientRequests>, String> {
    CLIENT_REQUESTS
        .get_or_init(|| ClientRequests::new().map(Mutex::new))
        .as_ref()
        .map_err(Clone::clone)?
        .lock()
        .map_err(|_| "Narration request registry lock poisoned".to_string())
}

pub(super) fn allocate_narration_job_id(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let kind = args["kind"]
        .as_str()
        .and_then(NarrationKind::parse)
        .ok_or("Invalid narration kind")?;
    let job_id = client_requests()?.allocate(kind)?;
    Ok(serde_json::json!({ "jobId": job_id }))
}

fn is_client_id(id: &str) -> bool {
    let Some(uuid) = id.strip_prefix("client-") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

pub(super) fn start_id(args: &serde_json::Value, prefix: &str) -> Result<String, String> {
    match args.get("jobId") {
        None => Ok(uuid(prefix)),
        Some(value) => value
            .as_str()
            .filter(|id| is_client_id(id) || id.starts_with("ticket-"))
            .map(str::to_owned)
            .ok_or_else(|| "Invalid narration jobId".to_string()),
    }
}

/// Caller holds the job registry lock until the new job has been inserted.
pub(super) fn claim_client_start(
    requests: &mut ClientRequests,
    job_id: &str,
    kind: NarrationKind,
) -> Result<(), String> {
    if requests.retire_ticket(job_id, kind, false)? {
        return Ok(());
    }
    if !is_client_id(job_id) {
        return Ok(());
    }
    if requests.legacy.contains_key(job_id) {
        return Err("Narration request already retired or completed".to_string());
    }
    if requests.legacy.len() >= MAX_CLIENT_REQUESTS {
        return Err("Narration request registry is full".to_string());
    }
    requests
        .legacy
        .insert(job_id.to_string(), ClientRequest::Started);
    Ok(())
}

/// Cancel a live job idempotently. Terminal and evicted jobs need no retry.
pub(super) fn cancel_job<S: JobState>(
    jobs: &HashMap<String, JobHandle<S>>,
    job_id: &str,
    kind: NarrationKind,
    update: impl FnOnce(&mut S),
) -> Result<serde_json::Value, String> {
    let Some(handle) = jobs.get(job_id) else {
        if job_id.starts_with("ticket-") {
            let mut requests = client_requests()?;
            // A stale worker epoch or an already-fenced ticket has no live job.
            requests.retire_ticket(job_id, kind, true)?;
        } else if is_client_id(job_id) {
            let mut requests = client_requests()?;
            if !requests.legacy.contains_key(job_id) && requests.legacy.len() >= MAX_CLIENT_REQUESTS
            {
                return Err("Narration request registry is full".to_string());
            }
            requests
                .legacy
                .insert(job_id.to_string(), ClientRequest::Retired);
        }
        return Ok(serde_json::json!({ "outcome": "missing" }));
    };
    let mut snapshot = handle
        .snapshot
        .lock()
        .map_err(|_| "Job snapshot lock poisoned")?;
    if !matches!(snapshot.state(), "queued" | "running") {
        return Ok(serde_json::json!({ "outcome": "terminal" }));
    }
    handle.cancelled.store(true, Ordering::SeqCst);
    update(&mut snapshot);
    Ok(serde_json::json!({ "outcome": "cancelled" }))
}

/// Builds a `{prefix}-{millis}-{pid}` job id. `millis` is the current Unix time
/// in milliseconds (same value the previous per-handler generators produced).
pub(super) fn uuid(prefix: &str) -> String {
    static NONCE: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Snapshot(&'static str);
    impl JobState for Snapshot {
        fn state(&self) -> &str {
            self.0
        }
    }

    #[test]
    fn cancellation_acknowledges_active_terminal_and_evicted_jobs() {
        let mut jobs = HashMap::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        jobs.insert(
            "job".to_string(),
            JobHandle::new(Arc::new(Mutex::new(Snapshot("running"))), cancelled.clone()),
        );
        let update = |snapshot: &mut Snapshot| snapshot.0 = "cancelled";
        assert_eq!(
            cancel_job(&jobs, "job", NarrationKind::Subtitle, update).unwrap()["outcome"],
            "cancelled"
        );
        assert!(cancelled.load(Ordering::SeqCst));
        assert_eq!(
            cancel_job(&jobs, "job", NarrationKind::Subtitle, update).unwrap()["outcome"],
            "terminal"
        );
        // The job thread (here: the test's clone) has exited.
        drop(cancelled);
        for index in 0..MAX_RETAINED_JOBS - 1 {
            jobs.insert(
                format!("terminal-{index:03}"),
                JobHandle::new(
                    Arc::new(Mutex::new(Snapshot("completed"))),
                    Arc::new(AtomicBool::new(false)),
                ),
            );
        }
        prepare_for_insert(&mut jobs).unwrap();
        assert!(!jobs.contains_key("job"));
        assert_eq!(
            cancel_job(&jobs, "job", NarrationKind::Subtitle, update).unwrap()["outcome"],
            "missing"
        );
    }

    #[test]
    fn cancelled_job_stays_active_until_its_thread_exits() {
        let mut jobs = HashMap::new();
        let thread_flag = Arc::new(AtomicBool::new(false));
        jobs.insert(
            "job".to_string(),
            JobHandle::new(
                Arc::new(Mutex::new(Snapshot("running"))),
                thread_flag.clone(),
            ),
        );
        cancel_job(&jobs, "job", NarrationKind::Subtitle, |snapshot| {
            snapshot.0 = "cancelled"
        })
        .unwrap();
        assert_eq!(find_active(&jobs).as_deref(), Some("job"));
        for index in 0..MAX_RETAINED_JOBS {
            jobs.insert(
                format!("terminal-{index:03}"),
                JobHandle::new(
                    Arc::new(Mutex::new(Snapshot("completed"))),
                    Arc::new(AtomicBool::new(false)),
                ),
            );
        }
        prepare_for_insert(&mut jobs).unwrap();
        assert!(
            jobs.contains_key("job"),
            "a winding-down job is not evicted"
        );
        drop(thread_flag);
        assert_eq!(find_active(&jobs), None);
    }

    #[test]
    fn cancellation_before_start_blocks_the_same_client_request() {
        let id = "client-11111111-2222-4333-8444-555555555555";
        let jobs: HashMap<String, JobHandle<Snapshot>> = HashMap::new();
        assert_eq!(
            cancel_job(&jobs, id, NarrationKind::Subtitle, |_| {}).unwrap()["outcome"],
            "missing"
        );
        let mut requests = client_requests().unwrap();
        assert!(claim_client_start(&mut requests, id, NarrationKind::Subtitle).is_err());
        let other = "client-11111111-2222-4333-8444-555555555556";
        assert!(claim_client_start(&mut requests, other, NarrationKind::Subtitle).is_ok());
        assert!(claim_client_start(&mut requests, other, NarrationKind::Subtitle).is_err());
    }

    #[test]
    fn start_ids_accept_valid_client_uuid_or_generate_legacy_ids() {
        let id = "client-aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
        assert_eq!(
            start_id(&serde_json::json!({"jobId": id}), "prefix").unwrap(),
            id
        );
        assert!(start_id(&serde_json::json!({"jobId": "../bad"}), "prefix").is_err());
        assert!(
            start_id(&serde_json::json!({}), "prefix")
                .unwrap()
                .starts_with("prefix-")
        );
    }

    #[test]
    fn tickets_allow_more_than_legacy_capacity_without_retaining_per_job_state() {
        let mut requests = ClientRequests::new().unwrap();
        let oldest = requests.allocate(NarrationKind::Subtitle).unwrap();
        requests
            .retire_ticket(&oldest, NarrationKind::Subtitle, true)
            .unwrap();
        for index in 0..MAX_CLIENT_REQUESTS + 100 {
            let ticket = requests.allocate(NarrationKind::Subtitle).unwrap();
            if index % 2 == 0 {
                claim_client_start(&mut requests, &ticket, NarrationKind::Subtitle).unwrap();
            } else {
                requests
                    .retire_ticket(&ticket, NarrationKind::Subtitle, true)
                    .unwrap();
            }
            assert!(claim_client_start(&mut requests, &ticket, NarrationKind::Subtitle).is_err());
        }
        assert!(claim_client_start(&mut requests, &oldest, NarrationKind::Subtitle).is_err());
        assert!(requests.legacy.is_empty());
        assert_eq!(
            requests.tickets[NarrationKind::Subtitle.index()].watermark,
            (MAX_CLIENT_REQUESTS + 101) as u64
        );
    }

    #[test]
    fn newer_cancellation_fences_delayed_start_but_not_other_kinds() {
        let mut requests = ClientRequests::new().unwrap();
        let old = requests.allocate(NarrationKind::Subtitle).unwrap();
        let newer = requests.allocate(NarrationKind::Subtitle).unwrap();
        let other = requests.allocate(NarrationKind::S2s).unwrap();
        requests
            .retire_ticket(&newer, NarrationKind::Subtitle, true)
            .unwrap();
        assert!(claim_client_start(&mut requests, &old, NarrationKind::Subtitle).is_err());
        assert!(claim_client_start(&mut requests, &newer, NarrationKind::Subtitle).is_err());
        assert!(claim_client_start(&mut requests, &other, NarrationKind::S2s).is_ok());
        assert!(
            requests
                .retire_ticket(&other, NarrationKind::Subtitle, true)
                .is_err()
        );
    }

    #[test]
    fn evicted_ticket_cannot_start_again() {
        let mut requests = ClientRequests::new().unwrap();
        let mut jobs = HashMap::new();
        let first = requests.allocate(NarrationKind::Subtitle).unwrap();
        for index in 0..MAX_RETAINED_JOBS {
            let id = if index == 0 {
                first.clone()
            } else {
                requests.allocate(NarrationKind::Subtitle).unwrap()
            };
            claim_client_start(&mut requests, &id, NarrationKind::Subtitle).unwrap();
            jobs.insert(
                id,
                JobHandle::new(
                    Arc::new(Mutex::new(Snapshot("completed"))),
                    Arc::new(AtomicBool::new(false)),
                ),
            );
        }
        prepare_for_insert(&mut jobs).unwrap();
        assert!(!jobs.contains_key(&first));
        assert!(claim_client_start(&mut requests, &first, NarrationKind::Subtitle).is_err());
    }

    #[test]
    fn a_full_registry_evicts_the_oldest_finished_job_first() {
        let mut requests = ClientRequests::new().unwrap();
        let tickets: Vec<String> = (0..100)
            .map(|_| requests.allocate(NarrationKind::Subtitle).unwrap())
            .collect();
        let mut jobs = HashMap::new();
        // Tickets 37..=100 finished in order; "-100" sorts before "-37" as text.
        for ticket in &tickets[36..] {
            jobs.insert(
                ticket.clone(),
                JobHandle::new(
                    Arc::new(Mutex::new(Snapshot("completed"))),
                    Arc::new(AtomicBool::new(false)),
                ),
            );
        }
        prepare_for_insert(&mut jobs).unwrap();
        assert!(
            !jobs.contains_key(&tickets[36]),
            "the oldest job goes first"
        );
        assert!(
            jobs.contains_key(&tickets[99]),
            "the job that just finished keeps its results"
        );
    }

    #[test]
    fn worker_restart_rejects_old_ticket_start_and_accepts_its_retirement() {
        let mut first = ClientRequests::new().unwrap();
        let old = first.allocate(NarrationKind::GeminiTranslate).unwrap();
        let mut restarted = ClientRequests::new().unwrap();
        assert!(claim_client_start(&mut restarted, &old, NarrationKind::GeminiTranslate).is_err());
        assert!(
            restarted
                .retire_ticket(&old, NarrationKind::GeminiTranslate, true)
                .unwrap()
        );
        let fresh = restarted.allocate(NarrationKind::GeminiTranslate).unwrap();
        assert!(claim_client_start(&mut restarted, &fresh, NarrationKind::GeminiTranslate).is_ok());
    }
}
