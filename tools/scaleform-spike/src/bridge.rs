//! The engine side of the Scaleform API: every `GameInterface.*`,
//! `CScaleformComponent_*.*` and `gameAPI.*` call the movies make lands here.
//! The spike answers with plausible local data; unknown calls return
//! `undefined`, as calling a missing native would.

use std::collections::BTreeMap;
use std::rc::Rc;

use ruffle_core::external::Value;

use crate::rewrite::Localization;

pub struct Bridge {
    pub loc: Rc<Localization>,
    pub root_ready: bool,
    /// Call counts by name, for the report.
    pub calls: BTreeMap<String, usize>,
    pub handled: BTreeMap<String, usize>,
    pub trace_calls: bool,
}

fn s(v: &str) -> Value {
    Value::String(v.to_string())
}

fn arg_str(args: &[Value], i: usize) -> String {
    match args.get(i) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

impl Bridge {
    pub fn new(loc: Rc<Localization>) -> Self {
        Self {
            loc,
            root_ready: false,
            calls: BTreeMap::new(),
            handled: BTreeMap::new(),
            trace_calls: std::env::var_os("SPIKE_TRACE_CALLS").is_some(),
        }
    }

    pub fn call(&mut self, name: &str, args: &[Value]) -> Value {
        *self.calls.entry(name.to_string()).or_default() += 1;
        let result = self.dispatch(name, args);
        if let Some(r) = &result {
            *self.handled.entry(name.to_string()).or_default() += 1;
            if self.trace_calls {
                tracing::info!("{name}{args:?} -> {r:?}");
            }
        } else if self.trace_calls {
            tracing::info!("{name}{args:?} -> (unhandled)");
        }
        result.unwrap_or(Value::Undefined)
    }

    fn dispatch(&mut self, name: &str, args: &[Value]) -> Option<Value> {
        let (object, method) = name.split_once('.').unwrap_or(("", name));
        Some(match (object, method) {
            ("csr", "RootReady") => {
                self.root_ready = true;
                Value::Undefined
            }
            (_, "Translate") => {
                let token = arg_str(args, 0);
                s(self.loc.translate_whole(&token).unwrap_or(&token))
            }
            _ => return None,
        })
    }
}
