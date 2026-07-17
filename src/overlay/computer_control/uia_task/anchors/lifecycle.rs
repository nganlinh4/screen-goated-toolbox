use super::*;

impl Brain {
    /// Discard bound anchors before a new capture. Only pending anchors survive.
    pub(in crate::overlay::computer_control::uia_task) fn invalidate_bound_anchors_for_new_frame(
        &mut self,
    ) {
        if anchors_bound_to_a_frame(&self.anchors) {
            self.clear_anchors("new_frame_invalidated_bound_anchors");
        }
    }

    /// Bind pending anchors once after the capture that first displays them.
    pub(in crate::overlay::computer_control::uia_task) fn bind_pending_anchors(
        &mut self,
        frame_id: u64,
        view: View,
        captured_surface: SurfaceIdentity,
    ) {
        if self.anchors.is_empty() {
            return;
        }
        let stable = super::super::frame_identity::validate_current(
            self.target.as_deref(),
            &captured_surface,
        )
        .is_ok();
        if !stable || !pending_anchors_match(&self.anchors, view, &captured_surface) {
            self.clear_anchors("pending_anchor_bind_rejected");
            return;
        }
        for anchor in &mut self.anchors {
            anchor.frame_id = frame_id;
        }
    }
}

fn anchors_bound_to_a_frame(anchors: &[ClickAnchor]) -> bool {
    anchors.iter().any(|anchor| anchor.frame_id != 0)
}

fn pending_anchors_match(anchors: &[ClickAnchor], view: View, surface: &SurfaceIdentity) -> bool {
    anchors.iter().all(|anchor| {
        anchor.frame_id == 0 && same_view(anchor.view, view) && surface == &anchor.surface
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(frame_id: u64, view: View, surface: SurfaceIdentity) -> ClickAnchor {
        ClickAnchor {
            id: 1,
            x: 25,
            y: 25,
            note: Some("control".into()),
            verify_description: None,
            source: AnchorSource::Detector,
            score: Some(0.9),
            bounds: Some([10, 10, 40, 40]),
            frame_id,
            view,
            surface,
        }
    }

    #[test]
    fn only_unbound_anchors_can_bind_to_a_new_frame() {
        let view = View {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let surface = SurfaceIdentity::Native {
            hwnd: 10,
            pid: 20,
            generation: 30,
        };
        let pending = anchor(0, view, surface.clone());
        assert!(!anchors_bound_to_a_frame(std::slice::from_ref(&pending)));
        assert!(pending_anchors_match(
            std::slice::from_ref(&pending),
            view,
            &surface
        ));

        let bound = anchor(7, view, surface.clone());
        assert!(anchors_bound_to_a_frame(std::slice::from_ref(&bound)));
        assert!(!pending_anchors_match(&[bound], view, &surface));
    }

    #[test]
    fn pending_bind_requires_the_same_view_and_concrete_surface() {
        let view = View {
            x: -20,
            y: 10,
            w: 100,
            h: 80,
        };
        let surface = SurfaceIdentity::Native {
            hwnd: 7,
            pid: 8,
            generation: 9,
        };
        let pending = anchor(0, view, surface.clone());
        assert!(!pending_anchors_match(
            std::slice::from_ref(&pending),
            View { x: -19, ..view },
            &surface
        ));
        let other = SurfaceIdentity::Native {
            hwnd: 10,
            pid: 11,
            generation: 12,
        };
        assert!(!pending_anchors_match(&[pending], view, &other));
    }

    #[test]
    fn pending_browser_anchor_binds_only_to_the_captured_document() {
        let view = View {
            x: 0,
            y: 0,
            w: 1280,
            h: 720,
        };
        let browser_surface = |tab_id, document_id: &str| SurfaceIdentity::Browser {
            tab_id,
            document_id: document_id.into(),
            window: super::super::super::super::controller::world::BrowserWindowIdentity {
                browser_window_id: 4,
                hwnd: 5,
                pid: 6,
                generation: 7,
            },
        };
        let captured = browser_surface(31, "document-a");
        let pending = anchor(0, view, captured.clone());

        assert!(pending_anchors_match(
            std::slice::from_ref(&pending),
            view,
            &captured
        ));
        assert!(!pending_anchors_match(
            std::slice::from_ref(&pending),
            view,
            &browser_surface(31, "document-b")
        ));
        assert!(!pending_anchors_match(
            &[pending],
            view,
            &browser_surface(32, "document-a")
        ));
    }
}
