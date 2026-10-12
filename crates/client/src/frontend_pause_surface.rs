//! Our passive visibility bookkeeping, not an authored game callback contract.
//! The caller enables this only in Match on its legacy Pause/Main page. Native
//! Options opening immediately suspends the panel; only an exact fresh sampler
//! reply can restore it. Register every OnLoadFinished, including cached paths.
use scaleform::Value;

pub(crate) const SAMPLER_PATH: &str = "_global.csrBindings";
pub(crate) const SAMPLER_METHOD: &str = "samplePauseSurface";
pub(crate) const RECEIVER: &str = "OwnedPauseSurface";
pub(crate) const CALLBACK: &str = "Visibility";

#[derive(Clone, Copy)]
pub(crate) enum Root {
    Pause,
    Options,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    owner: u64,
    epoch: u64,
    serial: u64,
    pause: String,
    options: String,
}
impl Request {
    pub(crate) fn args(&self) -> Vec<Value> {
        vec![
            self.owner.to_string().into(),
            self.epoch.to_string().into(),
            self.serial.to_string().into(),
            self.pause.clone().into(),
            self.options.clone().into(),
        ]
    }
}

#[derive(Default)]
pub(crate) struct Tracker {
    owner: Option<u64>,
    active: bool,
    epoch: u64,
    serial: u64,
    exhausted: bool,
    pause: Option<String>,
    options: Option<String>,
    options_opening: bool,
    pending: Option<Request>,
    last_frame: Option<u64>,
    visible: bool,
}
impl Tracker {
    pub(crate) fn options_root(&self) -> Option<&str> {
        self.options.as_deref()
    }

    /// Owner identifies the movie-worker lifetime, not the host or sim tick.
    /// Repeating identical state is a no-op; inactive always refuses sampling.
    pub(crate) fn activate(&mut self, owner: u64, active: bool) {
        if self.owner == Some(owner) && self.active == active {
            return;
        }
        if self.owner != Some(owner) {
            self.pause = None;
            self.options = None;
            self.options_opening = false;
            self.last_frame = None;
        }
        self.invalidate();
        self.owner = Some(owner);
        self.active = active && !self.exhausted;
    }

    /// A load callback is a new identity even if a cached level path is reused.
    /// Invalid handles forget the root and fail closed, never sample other data.
    pub(crate) fn root(&mut self, root: Root, handle: &str) {
        self.invalidate();
        let path = handle.strip_prefix("mc:").unwrap_or(handle);
        let value = root_path(path).then(|| path.to_string());
        match root {
            Root::Pause => self.pause = value,
            Root::Options => self.options = value,
        }
    }

    /// Call before requesting/showing Options, rather than waiting for OnReady.
    pub(crate) fn options_opened(&mut self) {
        self.invalidate();
        self.options_opening = true;
    }

    /// Desired Options lifecycle, independent of this overlay's active scope.
    /// The caller gates late Options OnReady/show on this flag even in Menu.
    pub(crate) fn options_requested(&self) -> bool {
        !self.exhausted && self.options_opening
    }

    /// Measured Cancel/owned scene teardown revokes a pending late OnReady.
    /// The caller hides its exact cached Options root; a fresh sampled reply
    /// is still required before this tracker restores Pause visibility.
    pub(crate) fn options_cancelled(&mut self) {
        self.invalidate();
        self.options_opening = false;
    }

    fn invalidate(&mut self) {
        self.visible = false;
        self.pending = None;
        if let Some(next) = self.epoch.checked_add(1) {
            self.epoch = next;
        } else {
            self.exhausted = true;
            self.active = false;
        }
    }

    /// Coalesce while a request is in flight and allow at most one request per
    /// actually presented movie frame, including root/Options epoch changes.
    pub(crate) fn request(&mut self, presented_frame: u64) -> Option<Request> {
        if !self.active
            || self.exhausted
            || self.pending.is_some()
            || self.last_frame.is_some_and(|last| presented_frame <= last)
        {
            return None;
        }
        let pause = self.pause.clone()?;
        let Some(serial) = self.serial.checked_add(1) else {
            self.exhausted = true;
            self.invalidate();
            return None;
        };
        self.serial = serial;
        self.last_frame = Some(presented_frame);
        let request = Request {
            owner: self.owner?,
            epoch: self.epoch,
            serial,
            pause,
            options: self.options.clone().unwrap_or_default(),
        };
        self.pending = Some(request.clone());
        Some(request)
    }

    /// EI reply: the five exact request strings, then Pause exists/visible/alpha
    /// and Options exists/visible/alpha. Never coerce undefined into false/zero.
    /// A matched but malformed reply closes visibility and releases the request;
    /// unrelated/late replies cannot consume the live request or restore state.
    pub(crate) fn accept(&mut self, args: &[Value]) -> bool {
        let Some(request) = self.pending.as_ref() else {
            return false;
        };
        if !self.active || args.len() < 5 || args[..5] != request.args()[..] {
            return false;
        }
        let request = self.pending.take().unwrap();
        self.visible = false;
        if args.len() != 11 {
            return true;
        }
        let pause = surface(&args[5..8]);
        let options_closed = if request.options.is_empty() {
            !self.options_opening
                && args[8..11] == [Value::Bool(false), Value::Undefined, Value::Undefined]
        } else {
            // Alpha zero alone does not disable Flash hit testing. Require an
            // actually hidden root, not merely a visually transparent overlay.
            surface(&args[8..11]).is_some_and(|(visible, _)| !visible)
        };
        self.visible = !self.options_opening
            && pause.is_some_and(|(visible, alpha)| visible && alpha == 100.)
            && options_closed;
        true
    }

    pub(crate) fn visible(&self) -> bool {
        self.active && !self.exhausted && self.visible
    }
}

fn root_path(path: &str) -> bool {
    let Some(level) = path.strip_prefix("_level") else {
        return false;
    };
    !level.is_empty()
        && level.len() <= 5
        && level.bytes().all(|byte| byte.is_ascii_digit())
        && (level == "0" || !level.starts_with('0'))
        && level.parse::<u16>().is_ok()
}
fn surface(args: &[Value]) -> Option<(bool, f64)> {
    let [
        Value::Bool(true),
        Value::Bool(visible),
        Value::Number(alpha),
    ] = args
    else {
        return None;
    };
    (alpha.is_finite() && (0. ..=100.).contains(alpha)).then_some((*visible, *alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> Tracker {
        let mut tracker = Tracker::default();
        tracker.activate(23, true);
        tracker.root(Root::Pause, "mc:_level100");
        tracker
    }
    fn reply(request: &Request, pause: bool, options: Option<bool>) -> Vec<Value> {
        let mut args = request.args();
        args.extend([true.into(), pause.into(), 100.0.into()]);
        args.extend(match options {
            Some(visible) => [true.into(), visible.into(), 100.0.into()],
            None => [false.into(), Value::Undefined, Value::Undefined],
        });
        args
    }
    fn prove(tracker: &mut Tracker, frame: u64, options: Option<bool>) {
        let request = tracker.request(frame).unwrap();
        assert!(tracker.accept(&reply(&request, true, options)));
    }

    #[test]
    fn requires_active_exact_presented_pause_and_strict_primitive_proof() {
        let mut t = tracker();
        assert!(!t.visible());
        prove(&mut t, 1, None);
        assert!(t.visible());
        for invalid in [
            Value::Undefined,
            Value::Number(f64::NAN),
            Value::Number(0.),
            Value::Number(50.),
            Value::Number(101.),
        ] {
            let request = t.request(t.serial + 1).unwrap();
            let mut args = reply(&request, true, None);
            args[7] = invalid;
            t.accept(&args);
            assert!(!t.visible());
        }
        t.activate(23, false);
        assert!(t.request(100).is_none());
        assert!(!t.visible());
    }

    #[test]
    fn options_suspend_before_load_and_restore_only_exact_hidden_root() {
        let mut t = tracker();
        prove(&mut t, 1, None);
        let old = t.request(2).unwrap();
        t.options_opened();
        assert!(!t.visible());
        assert!(!t.accept(&reply(&old, true, None)));
        prove(&mut t, 3, None);
        assert!(!t.visible());
        t.root(Root::Options, "mc:_level21");
        prove(&mut t, 4, Some(true));
        assert!(!t.visible());
        let request = t.request(5).unwrap();
        let mut args = reply(&request, true, Some(true));
        args[10] = 0.0.into();
        t.accept(&args);
        assert!(!t.visible(), "Transparent Options still owns hit testing");
        prove(&mut t, 6, Some(false));
        assert!(
            !t.visible(),
            "Cached hidden root cannot finish a wanted opening"
        );
        assert!(t.options_requested());
        t.options_cancelled();
        assert!(!t.options_requested());
        assert!(!t.visible(), "Cancel still requires fresh root proof");
        prove(&mut t, 7, Some(false));
        assert!(t.visible());
        let request = t.request(8).unwrap();
        t.accept(&reply(&request, false, Some(false)));
        assert!(!t.visible());
    }

    #[test]
    fn cancel_before_ready_and_inactive_scope_preserve_only_owned_lifecycle() {
        let mut t = tracker();
        t.activate(23, false);
        t.options_opened();
        assert!(
            t.options_requested(),
            "Wanted Options is independent of Pause scope"
        );
        t.activate(23, true);
        let opening = t.request(1).unwrap();
        t.options_cancelled();
        assert!(!t.options_requested());
        assert!(!t.accept(&reply(&opening, true, None)));
        prove(&mut t, 2, None);
        assert!(t.visible(), "Cancel before cold load must not hang Pause");
        // The caller sees wanted=false, so late OnReady hides its exact root.
        // A new loader identity still invalidates the outgoing proof.
        t.root(Root::Options, "_level21");
        assert!(!t.visible());
        prove(&mut t, 3, Some(false));
        assert!(t.visible());
        t.activate(23, false);
        t.activate(23, true);
        prove(&mut t, 4, Some(false));
        assert!(
            t.visible(),
            "Scope transitions keep this worker's cached roots"
        );
    }

    #[test]
    fn same_path_reload_owner_and_inactive_epochs_reject_old_callbacks() {
        let mut t = tracker();
        let old = t.request(1).unwrap();
        t.root(Root::Pause, "mc:_level100");
        let live = t.request(2).unwrap();
        assert!(!t.accept(&reply(&old, true, None)));
        assert!(t.accept(&reply(&live, true, None)));
        let old = t.request(3).unwrap();
        t.activate(24, true);
        assert!(!t.accept(&reply(&old, true, None)));
        assert!(t.request(4).is_none(), "New owner must register its roots");
        t.root(Root::Pause, "_level100");
        let old = t.request(4).unwrap();
        t.activate(24, false);
        t.activate(24, true);
        let live = t.request(5).unwrap();
        assert!(!t.accept(&reply(&old, true, None)));
        assert!(t.accept(&reply(&live, true, None)));
    }

    #[test]
    fn requests_coalesce_per_presented_frame_and_stale_roots_never_consume_live() {
        let mut t = tracker();
        let request = t.request(10).unwrap();
        for _ in 0..1000 {
            assert!(t.request(11).is_none());
        }
        let mut wrong = reply(&request, true, None);
        wrong[3] = "_level99".into();
        assert!(!t.accept(&wrong));
        assert!(t.accept(&reply(&request, true, None)));
        assert!(!t.accept(&reply(&request, true, None)));
        assert!(t.request(10).is_none());
        t.options_opened();
        assert!(
            t.request(10).is_none(),
            "Epoch changes cannot flood a frame"
        );
        assert!(t.request(11).is_some());
    }

    #[test]
    fn malformed_and_missing_root_proof_fail_closed_without_boolean_coercion() {
        let mut t = tracker();
        t.root(Root::Options, "_level21");
        for (index, invalid) in [
            (5, Value::Undefined),
            (6, Value::String("true".into())),
            (8, Value::Bool(false)),
            (9, Value::Undefined),
            (10, Value::Number(f64::INFINITY)),
        ] {
            let request = t.request(t.serial + 1).unwrap();
            let mut args = reply(&request, true, Some(false));
            args[index] = invalid;
            assert!(t.accept(&args));
            assert!(!t.visible());
        }
        let request = t.request(100).unwrap();
        assert!(t.accept(&request.args()));
        assert!(!t.visible());
        t.root(Root::Pause, "_global.anything");
        assert!(t.request(101).is_none());
    }

    #[test]
    fn counter_exhaustion_cannot_reuse_receipts() {
        let mut t = tracker();
        prove(&mut t, 1, None);
        t.epoch = u64::MAX;
        t.options_opened();
        assert!(!t.visible());
        assert!(t.request(2).is_none());
        t.activate(25, true);
        assert!(t.request(3).is_none());
        let mut t = tracker();
        t.serial = u64::MAX;
        assert!(t.request(1).is_none());
        assert!(!t.visible());
    }
}
