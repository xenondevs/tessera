use crate::diagnostics::Diagnostics;
use crate::resource::ResourceId;
use crate::scene::blockstate::StateQuery;
use tessera_capture::model::{Block, Draw, Item, Material, RenderShape};

#[derive(Copy, Clone)]
pub struct Capture<'a> {
    pub materials: &'a [&'a Material<'a>],
    pub draws: &'a [Draw<'a>],
}

#[cfg_attr(not(feature = "embedded-assets"), allow(unused_variables))]
pub fn lookup_block(block: &ResourceId, query: &StateQuery, diag: &Diagnostics) -> Option<(RenderShape, Capture<'static>)> {
    cfg_select! {
        feature = "embedded-assets" => find(&super::tables::BLOCKS, block, query, diag),
        _ => None,
    }
}

#[cfg_attr(not(feature = "embedded-assets"), allow(unused_variables))]
pub fn lookup_item(item: &ResourceId) -> Option<Capture<'static>> {
    cfg_select! {
        feature = "embedded-assets" => find_item(&super::tables::ITEMS, item),
        _ => None,
    }
}

pub fn find<'a>(
    blocks: &[&'a Block<'a>],
    block: &ResourceId,
    query: &StateQuery,
    diag: &Diagnostics,
) -> Option<(RenderShape, Capture<'a>)> {
    let wanted = (block.namespace.as_ref(), block.path.as_ref());
    let entry = blocks[blocks
        .binary_search_by(|entry| (entry.namespace, entry.path).cmp(&wanted))
        .ok()?];

    let mut key = String::new();
    for prop in entry.properties {
        let Some(value) = query.get(prop) else {
            diag.warn(block, || format!("State query doesn't have a property {prop}"));
            return None;
        };
        if !key.is_empty() {
            key.push(',')
        }
        key.push_str(prop);
        key.push('=');
        key.push_str(value);
    }

    let idx = entry
        .states
        .binary_search_by(|state| (*state.key).cmp(key.as_str()))
        .ok()?;
    let state = &entry.states[idx];

    Some((state.render_shape, Capture { materials: entry.materials, draws: state.draws }))
}

pub fn find_item<'a>(items: &[&'a Item<'a>], item: &ResourceId) -> Option<Capture<'a>> {
    let wanted = (item.namespace.as_ref(), item.path.as_ref());
    let entry = items[items
        .binary_search_by(|entry| (entry.namespace, entry.path).cmp(&wanted))
        .ok()?];

    Some(Capture { materials: entry.materials, draws: entry.draws })
}
