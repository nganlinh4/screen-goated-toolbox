//! Exact target adoption when a provider becomes available during a live turn.

use super::world::SurfaceIdentity;

impl super::Controller {
    /// Pin the browser surface that produced the cached world without discarding
    /// its IDs. A different or stale observation can never become the turn target.
    pub fn adopt_observed_browser_target(&mut self, identity: &SurfaceIdentity) -> bool {
        let matches_observation = self
            .last
            .as_ref()
            .is_some_and(|world| &world.identity == identity);
        let SurfaceIdentity::Browser { tab_id, .. } = identity else {
            return false;
        };
        if self.browser_tab_id.is_none() && matches_observation {
            self.browser_tab_id = Some(*tab_id);
            true
        } else {
            self.browser_tab_id == Some(*tab_id) && matches_observation
        }
    }
}
