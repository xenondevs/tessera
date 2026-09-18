use super::tables::BLOCKS;
use crate::diagnostics::Diagnostics;
use crate::resource::ResourceId;
use crate::scene::blockstate::StateQuery;
use tessera_capture_gen::model::{Block, Material, State};

#[derive(Copy, Clone)]
pub struct Capture<'a> {
    pub block: &'a Block<'a>,
    pub state: &'a State<'a>,
}

impl<'a> Capture<'a> {
    pub fn material(self, local: u8) -> &'a Material<'a> {
        self.block.materials[usize::from(local)]
    }
}

pub fn lookup(block: &ResourceId, query: &StateQuery, diag: &Diagnostics) -> Option<Capture<'static>> {
    find(&BLOCKS, block, query, diag)
}

pub fn find<'a>(blocks: &[&'a Block<'a>], block: &ResourceId, query: &StateQuery, diag: &Diagnostics) -> Option<Capture<'a>> {
    let wanted = (block.namespace.as_ref(), block.path.as_ref());
    let entry = blocks[blocks.binary_search_by(|entry| (entry.namespace, entry.path).cmp(&wanted)).ok()?];

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

    let idx = entry.states.binary_search_by(|state| (*state.key).cmp(key.as_str())).ok()?;
    Some(Capture { block: entry, state: &entry.states[idx] })
}
