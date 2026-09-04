use crate::diagnostics::Diagnostics;
use crate::resource::texture::TextureSlot;
use crate::util::FastHashMap;
use foldhash::HashMapExt;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(from = "TextureSlot")]
pub enum SlotContents {
    Value(TextureSlot),
    Reference(String),
}

impl From<TextureSlot> for SlotContents {
    fn from(mut value: TextureSlot) -> Self {
        if value.sprite.starts_with('#') {
            value.sprite.remove(0);
            Self::Reference(value.sprite)
        } else {
            Self::Value(value)
        }
    }
}

pub fn resolve_slots(
    chain: &[&FastHashMap<String, SlotContents>],
    subject: &str,
    diag: &Diagnostics,
) -> FastHashMap<String, TextureSlot> {
    let mut resolved: FastHashMap<&str, &TextureSlot> = FastHashMap::new();
    let mut unresolved: FastHashMap<&str, &str> = FastHashMap::new();

    for layer in chain.iter().rev() {
        for (slot, contents) in *layer {
            match contents {
                SlotContents::Value(material) => {
                    unresolved.remove(slot.as_str());
                    resolved.insert(slot, material);
                }
                SlotContents::Reference(name) => {
                    resolved.remove(slot.as_str());
                    unresolved.insert(slot, name);
                }
            }
        }
    }

    let mut out = FastHashMap::with_capacity(resolved.len() + unresolved.len());
    let max_hops = unresolved.len() + 1;
    let mut still: Vec<(&str, &str)> = Vec::new();
    for (&name, &start) in unresolved.iter() {
        let (mut target, mut hops) = (start, 0);
        loop {
            if let Some(&m) = resolved.get(target) {
                out.insert(name.to_string(), m.clone());
                break;
            }
            match unresolved.get(target) {
                Some(&next) if hops < max_hops => {
                    target = next;
                    hops += 1;
                }
                _ => {
                    still.push((name, start));
                    break;
                }
            }
        }
    }

    if !still.is_empty() {
        diag.warn(subject, || {
            let mut list = still.into_iter().map(|(k, v)| format!("#{k} -> #{v}")).collect::<Vec<_>>();
            list.sort();
            format!("Unresolved texture references: {}", list.join(", "))
        });
    }

    for (k, v) in resolved {
        out.insert(k.to_string(), v.clone());
    }

    out
}
