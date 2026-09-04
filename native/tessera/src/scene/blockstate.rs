use crate::resource::blockstate::{BlockStateModel, Condition, ModelPart, Term};
use crate::util::FastHashMap;

pub struct StateQuery<'a> {
    pub(crate) pairs: FastHashMap<&'a str, &'a str>,
}

impl<'a> StateQuery<'a> {
    pub fn parse(query: &'a str) -> Self {
        let query = query.trim();
        let query = query.strip_suffix(']').unwrap_or(query);
        let query = query.find('[').map_or(query, |i| &query[i + 1..]);
        let pairs = query
            .split(',')
            .filter(|p| !p.trim().is_empty())
            .filter_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                Some((key.trim(), value.trim()))
            })
            .collect();
        Self { pairs }
    }

    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.pairs.get(key).copied()
    }
}

impl Condition {
    pub fn matches(&self, query: &StateQuery) -> bool {
        match self {
            Condition::Or(subs) => subs.iter().any(|sub| sub.matches(query)),
            Condition::And(subs) => subs.iter().all(|sub| sub.matches(query)),
            Condition::Terms(terms) => terms.iter().all(|term| term_matches(term, query)),
        }
    }
}

fn term_matches(term: &Term, query: &StateQuery) -> bool {
    let Some(actual) = query.get(&term.property) else {
        return false;
    };
    term.alternatives.split('|').any(|alt| match alt.strip_prefix('!') {
        Some(value) => actual != value,
        None => actual == alt,
    })
}

pub fn parts<'a>(model: &'a BlockStateModel, props: &StateQuery) -> Vec<&'a ModelPart> {
    match model {
        BlockStateModel::Variants(variants) => variants
            .iter()
            .find(|(key, _)| key.iter().all(|(k, v)| props.get(k) == Some(v)))
            .and_then(|(_, weighted)| weighted.first())
            .map(|(part, _)| vec![part])
            .unwrap_or_default(),
        BlockStateModel::Multipart(parts) => parts
            .iter()
            .filter(|(cond, _)| cond.as_ref().is_none_or(|c| c.matches(props)))
            .filter_map(|(_, weighted)| weighted.first())
            .map(|(part, _)| part)
            .collect(),
    }
}
