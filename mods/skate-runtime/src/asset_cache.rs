// SPDX-License-Identifier: GPL-3.0-only
//! Immutable inputs from the admitted, frozen private bundle. The provider has
//! one simulation owner; cached parsing never carries player state across ticks.
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::Arc,
};
struct Assets {
    root: PathBuf,
    collections: Option<Arc<skate_data::collections::Collections>>,
    banks: Option<Arc<skate_data::animation_banks::AnimationBanks>>,
}
thread_local! {static DATA:RefCell<Option<Assets>>=const{RefCell::new(None)};}
fn access<T>(
    root: &Path,
    read: impl FnOnce(&mut Assets) -> Result<T, String>,
) -> Result<T, String> {
    DATA.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_none_or(|a| a.root != root) {
            *slot = Some(Assets {
                root: root.into(),
                collections: None,
                banks: None,
            });
        }
        read(slot.as_mut().unwrap())
    })
}
pub(crate) fn collections(
    root: &Path,
) -> Result<Arc<skate_data::collections::Collections>, String> {
    access(root, |a| {
        if a.collections.is_none() {
            a.collections = Some(Arc::new(skate_data::collections::Collections::load(root)?));
        }
        Ok(a.collections.as_ref().unwrap().clone())
    })
}
pub(crate) fn banks(
    root: &Path,
) -> Result<Arc<skate_data::animation_banks::AnimationBanks>, String> {
    access(root, |a| {
        if a.banks.is_none() {
            a.banks = Some(Arc::new(skate_data::animation_banks::AnimationBanks::load(
                root,
            )?));
        }
        Ok(a.banks.as_ref().unwrap().clone())
    })
}
