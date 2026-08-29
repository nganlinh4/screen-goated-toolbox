//! UIA-grounded Computer Control primitives shared by the visible runtime.

use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tungstenite::Message;

use crate::api::gemini_live::transport::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};

use super::executor;
use super::grid::Grid;
use super::human_input::{self, HumanProfile};
use super::protocol::{ServerEvent, parse_server_message};
use super::session::{self, Sock, View, connect_ws, send};
use super::uia::{self, UiElement};

mod anchors;
mod brain;
mod browser_dispatch;
mod dispatch;
mod dispatch_guard;
mod dispatch_telemetry;
mod exact_edit_guard;
mod executable_provenance;
mod frame_identity;
mod harness_options;
mod keyboard_target_gate;
mod perception;
mod postcondition;
mod prompt;
mod receipts;
mod render;
mod resource_authorization;
mod setup_guard;
mod snapshot;
mod structural_authorization;
mod structural_edit;
mod tab_ownership;
#[cfg(test)]
mod turn_state_tests;
mod vision;
mod vision_verify;
use anchors::*;
pub(in crate::overlay::computer_control) use frame_identity::FrameSource;
use perception::*;
use postcondition::*;
pub(crate) use prompt::build_setup;
use render::*;
pub(super) use snapshot::{SnapshotFrame, snapshot};
use vision::*;
use vision_verify::*;

/// Reconnect the Live session, resuming the prior conversation by `resume` handle.
pub(super) fn reconnect(
    key: &str,
    resume: Option<&str>,
    voice: bool,
    search: bool,
    reconnect_context: Option<&str>,
) -> Result<Sock> {
    let mut socket = connect_ws(key).context("reconnect")?;
    let setup = prompt::build_setup_with_context(resume, voice, search, reconnect_context)?;
    super::telemetry::record_model_setup(&setup, "reconnect");
    send(&mut socket, setup)?;
    wait_for_setup(&mut socket)?;
    set_socket_nonblocking(&mut socket)?;
    Ok(socket)
}

/// The shared agent state and grounded tool dispatcher. One executor thread owns
/// it, while the runtime reader continues receiving audio and cancellation.
pub(super) struct Brain {
    pub dir: String,
    grid: Grid,
    profile: HumanProfile,
    dry: bool,
    pub target: Option<String>,
    pub view: View,
    zoomed: bool,
    whole_screen: bool,
    last_click: Option<(i32, i32)>,
    pub step: usize,
    active_action: Option<super::telemetry::ActionTrace>,
    current_turn_id: Option<u64>,
    source_frame: Option<FrameSource>,
    controlled_tab_id: Option<i64>,
    controlled_document_id: Option<String>,
    turn_tabs: tab_ownership::TurnTabOwnership,
    recent_actions: Vec<String>,
    advice_latches: Vec<String>,
    prev_state_sig: Option<String>,
    click_before: Option<Vec<u8>>,
    trail: Vec<String>,
    exact_edit_guard: exact_edit_guard::ExactEditGuard,
    structural_authorization: structural_authorization::StructuralAuthorization,
    resource_authorization: resource_authorization::ResourceAuthorization,
    wait_accum: f64,
    anchors: Vec<ClickAnchor>,
    next_anchor_id: u32,
    controller: super::controller::Controller,
    show_coarse_grid: bool,
    setup_guard: setup_guard::SetupGuard,
    keyboard_target_gate: keyboard_target_gate::KeyboardTargetGate,
}

pub(super) struct Grounded {
    pub frame_b64: String,
    pub source: FrameSource,
    pub state_text: String,
    pub postcondition: GroundPostcondition,
}

struct SemanticSurfaceState {
    elements: String,
    title: String,
    url: String,
    identity: super::controller::world::SurfaceIdentity,
}
