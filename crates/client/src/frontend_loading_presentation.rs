//! A loading raster must reach the main image before synchronous scene imports.
//! Reuses the movie worker's captured presentation revision; this is an app /
//! render opportunity fence, not a claim about monitor scanout.
#[derive(Default)]
pub(crate) struct Presentation {
    serial: Option<u64>,
    revision: Option<u64>,
    assigned_frame: Option<u32>,
}
impl Presentation {
    pub(crate) fn select(&mut self, serial: Option<u64>) {
        if self.serial != serial {
            *self = Self {
                serial,
                ..Self::default()
            };
        }
    }
    pub(crate) fn awaiting_init(&self) -> bool {
        self.serial.is_some() && self.revision.is_none() && self.assigned_frame.is_none()
    }
    pub(crate) fn request(&mut self, revision: u64) {
        self.revision = Some(revision);
    }
    pub(crate) fn assigned(&mut self, revision: u64, frame: u32) -> bool {
        if self.revision == Some(revision) && self.assigned_frame.is_none() {
            self.assigned_frame = Some(frame);
            return true;
        }
        false
    }
    pub(crate) fn fallback(&mut self, frame: u32) {
        if self.assigned_frame.is_none() {
            self.assigned_frame = Some(frame);
        }
    }
    pub(crate) fn ready(&self, serial: u64, frame: u32) -> bool {
        self.serial == Some(serial)
            && self
                .assigned_frame
                .is_some_and(|assigned| assigned != frame)
    }
}

/// Successful art has no timer minimum. This deadline only escapes a missing
/// callback/worker so map entry cannot remain Loading forever.
pub(crate) fn waiting(painted: bool, elapsed: std::time::Duration) -> bool {
    !painted && elapsed < std::time::Duration::from_secs(5)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_movie_has_a_finite_escape_and_success_has_no_minimum_delay() {
        use std::time::Duration;
        assert!(!waiting(true, Duration::ZERO));
        assert!(waiting(false, Duration::from_millis(4999)));
        assert!(!waiting(false, Duration::from_secs(5)));
        assert!(!waiting(false, Duration::from_secs(60)));
    }
    #[test]
    fn actual_matching_raster_requires_a_completed_app_iteration() {
        let mut gate = Presentation::default();
        gate.select(Some(9));
        gate.request(41);
        gate.assigned(40, 17); // An older readback finishes after the new request.
        assert!(!gate.ready(9, 18));
        gate.assigned(41, 18);
        assert!(!gate.ready(9, 18));
        assert!(gate.ready(9, 19));
        assert!(!gate.ready(10, 19));
    }
    #[test]
    fn cancel_and_reselect_revoke_old_callbacks_and_rasters() {
        let mut gate = Presentation::default();
        gate.select(Some(1));
        gate.request(4);
        gate.select(None);
        gate.assigned(4, 10);
        assert!(!gate.ready(1, 11));
        gate.select(Some(2));
        gate.assigned(4, 12);
        assert!(!gate.ready(2, 13));
        assert!(gate.awaiting_init());
        gate.request(5);
        gate.assigned(5, 13);
        assert!(gate.ready(2, 14));
    }
    #[test]
    fn unsupported_art_fallback_still_gets_one_iteration_including_frame_wrap() {
        let mut gate = Presentation::default();
        gate.select(Some(7));
        gate.fallback(u32::MAX);
        assert!(!gate.ready(7, u32::MAX));
        assert!(gate.ready(7, 0));
        gate.select(Some(8));
        assert!(!gate.ready(8, 0));
    }
}
