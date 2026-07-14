//! Surface identity carried with every frame visible to the acting model.

use anyhow::Result;
use serde_json::{Value, json};

use super::super::controller::world::SurfaceIdentity;
use super::{View, session, uia};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(in crate::overlay::computer_control) struct FrameSource {
    pub frame_id: u64,
    pub surface: SurfaceIdentity,
}

impl FrameSource {
    pub(super) fn native(frame_id: u64, identity: (u64, u64, u64)) -> Self {
        Self {
            frame_id,
            surface: SurfaceIdentity::Native {
                hwnd: identity.0,
                pid: identity.1,
                generation: identity.2,
            },
        }
    }

    pub(super) fn native_identity(&self) -> Option<(u64, u64, u64)> {
        match self.surface {
            SurfaceIdentity::Native {
                hwnd,
                pid,
                generation,
            } => Some((hwnd, pid, generation)),
            SurfaceIdentity::Browser { .. } => None,
        }
    }

    pub(super) fn input_guard(&self) -> Value {
        match &self.surface {
            SurfaceIdentity::Native {
                hwnd,
                pid,
                generation,
            } => json!({
                "kind": "native", "hwnd": hwnd, "pid": pid, "generation": generation,
            }),
            SurfaceIdentity::Browser {
                tab_id,
                document_id,
                window,
            } => json!({
                "kind": "browser", "tab_id": tab_id, "document_id": document_id,
                "browser_window_id": window.browser_window_id,
                "hwnd": window.hwnd, "pid": window.pid, "generation": window.generation,
            }),
        }
    }
}

pub(super) fn capture_current(
    target: Option<&str>,
    fixed: Option<(&SurfaceIdentity, View)>,
    resolve_view: impl FnOnce() -> View,
) -> Result<(session::Capture, SurfaceIdentity, View, bool)> {
    let surface = observe_current(target)?;
    let dynamic_view = resolve_view();
    let (view, fixed_retained) = choose_bound_view(&surface, fixed, dynamic_view);
    let capture = session::capture_virtual()?;
    validate_current(target, &surface)?;
    Ok((capture, surface, view, fixed_retained))
}

fn choose_bound_view(
    observed: &SurfaceIdentity,
    fixed: Option<(&SurfaceIdentity, View)>,
    dynamic: View,
) -> (View, bool) {
    match fixed {
        Some((expected, view)) if expected == observed => (view, true),
        _ => (dynamic, false),
    }
}

fn observe_current(target: Option<&str>) -> Result<SurfaceIdentity> {
    if super::super::browser::input_active() {
        let (tab_id, document_id, window) = super::super::browser::active_document_identity()?;
        Ok(SurfaceIdentity::Browser {
            tab_id,
            document_id,
            window,
        })
    } else {
        let (hwnd, pid, generation) = uia::observe_native_identity(target)?;
        Ok(SurfaceIdentity::Native {
            hwnd,
            pid,
            generation,
        })
    }
}

pub(super) fn validate_current(target: Option<&str>, expected: &SurfaceIdentity) -> Result<()> {
    match expected {
        SurfaceIdentity::Native {
            hwnd,
            pid,
            generation,
        } => {
            uia::validate_native_identity(*hwnd, *pid, *generation)?;
            let actual = uia::current_native_identity(target)?;
            if actual != (*hwnd, *pid, *generation) {
                anyhow::bail!("native surface changed after the model-visible frame");
            }
            Ok(())
        }
        SurfaceIdentity::Browser {
            tab_id,
            document_id,
            window,
        } => super::super::browser::validate_active_document_identity(*tab_id, document_id, window),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_window() -> crate::overlay::computer_control::controller::world::BrowserWindowIdentity
    {
        crate::overlay::computer_control::controller::world::BrowserWindowIdentity {
            browser_window_id: 2,
            hwnd: 3,
            pid: 4,
            generation: 5,
        }
    }

    #[test]
    fn input_guard_preserves_captured_identity_without_foreground_rebinding() {
        let native = FrameSource {
            frame_id: 4,
            surface: SurfaceIdentity::Native {
                hwnd: 17,
                pid: 29,
                generation: 3,
            },
        };
        assert_eq!(native.input_guard()["hwnd"], 17);
        assert_eq!(native.input_guard()["generation"], 3);

        let browser = FrameSource {
            frame_id: 5,
            surface: SurfaceIdentity::Browser {
                tab_id: 41,
                document_id: "doc-a".into(),
                window: browser_window(),
            },
        };
        assert_eq!(browser.input_guard()["tab_id"], 41);
        assert_eq!(browser.input_guard()["document_id"], "doc-a");
        assert_eq!(browser.input_guard()["hwnd"], 3);
    }

    #[test]
    fn stale_fixed_crop_falls_back_to_view_resolved_inside_new_surface_bracket() {
        let old = SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 3,
        };
        let new = SurfaceIdentity::Native {
            hwnd: 4,
            pid: 5,
            generation: 6,
        };
        let fixed = View {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };
        let dynamic = View {
            x: 50,
            y: 60,
            w: 70,
            h: 80,
        };

        assert_eq!(
            choose_bound_view(&old, Some((&old, fixed)), dynamic),
            (fixed, true)
        );
        assert_eq!(
            choose_bound_view(&new, Some((&old, fixed)), dynamic),
            (dynamic, false)
        );
    }
}
