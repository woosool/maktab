//! A small, capped cache of pictures made on the fly (gallery tiles, live previews): the
//! least recently used are freed, so typing in an editor or dragging a control doesn't
//! keep every intermediate picture alive.

use std::collections::VecDeque;

const MAX: usize = 120;

#[derive(Clone, Default)]
struct Cache(std::sync::Arc<std::sync::Mutex<VecDeque<(String, egui::TextureHandle)>>>);

fn cache(ctx: &egui::Context) -> Cache {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(egui::Id::new("sard-texcache")).clone())
}

/// The picture for `key`, made by `make` the first time.
pub fn get_or<E>(ctx: &egui::Context, key: &str, make: impl FnOnce() -> Result<egui::TextureHandle, E>) -> Result<egui::TextureHandle, E> {
    let c = cache(ctx);
    {
        let mut q = c.0.lock().unwrap();
        if let Some(i) = q.iter().position(|e| e.0 == key) {
            let e = q.remove(i).unwrap();
            let t = e.1.clone();
            q.push_back(e);
            return Ok(t);
        }
    }
    let t = make()?;
    let mut q = c.0.lock().unwrap();
    q.push_back((key.to_string(), t.clone()));
    while q.len() > MAX { q.pop_front(); }
    Ok(t)
}

/// Free everything (a new presentation, a closed editor).
pub fn clear(ctx: &egui::Context) { cache(ctx).0.lock().unwrap().clear(); }

#[cfg(test)]
pub fn len(ctx: &egui::Context) -> usize { cache(ctx).0.lock().unwrap().len() }
