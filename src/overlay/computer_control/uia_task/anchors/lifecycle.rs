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
        captured_surface: Option<SurfaceIdentity>,
    ) {
        if self.anchors.is_empty() {
            return;
        }
        let current_surface = current_surface_identity();
        let stable_surface = captured_surface.filter(|surface| current_surface == Some(*surface));
        if !pending_anchors_match(&self.anchors, view, stable_surface) {
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

fn pending_anchors_match(
    anchors: &[ClickAnchor],
    view: View,
    surface: Option<SurfaceIdentity>,
) -> bool {
    anchors.iter().all(|anchor| {
        anchor.frame_id == 0 && same_view(anchor.view, view) && surface == Some(anchor.surface)
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
        let surface = (10, 20, 30);
        let pending = anchor(0, view, surface);
        assert!(!anchors_bound_to_a_frame(std::slice::from_ref(&pending)));
        assert!(pending_anchors_match(
            std::slice::from_ref(&pending),
            view,
            Some(surface)
        ));

        let bound = anchor(7, view, surface);
        assert!(anchors_bound_to_a_frame(std::slice::from_ref(&bound)));
        assert!(!pending_anchors_match(&[bound], view, Some(surface)));
    }

    #[test]
    fn pending_bind_requires_the_same_view_and_concrete_surface() {
        let view = View {
            x: -20,
            y: 10,
            w: 100,
            h: 80,
        };
        let pending = anchor(0, view, (7, 8, 9));
        assert!(!pending_anchors_match(
            std::slice::from_ref(&pending),
            View { x: -19, ..view },
            Some((7, 8, 9))
        ));
        assert!(!pending_anchors_match(&[pending], view, None));
    }
}
