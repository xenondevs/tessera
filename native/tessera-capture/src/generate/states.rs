use super::GenerateError;
use crate::raw::RawBlockFile;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(super) struct StateGrid<'a> {
    pub(super) properties: Vec<&'a str>,
    rows: Vec<Vec<&'a str>>,
}

impl<'a> StateGrid<'a> {
    pub(super) fn parse(file: &'a RawBlockFile, path: &Path) -> Result<Self, GenerateError> {
        let invalid = |message: String| GenerateError::Invalid { path: path.to_owned(), message };
        let mut props = Vec::new();
        let mut rows = Vec::with_capacity(file.states.len());

        for key in file.states.keys() {
            let mut row = Vec::with_capacity(props.len());

            for (column, pair) in key.split(',').filter(|pair| !pair.is_empty()).enumerate() {
                let (prop, value) = pair
                    .split_once('=')
                    .ok_or_else(|| invalid(format!("malformed state key {key}")))?;
                if rows.is_empty() {
                    props.push(prop)
                } else if props.get(column) != Some(&prop) {
                    return Err(invalid(format!(
                        "state key {key} has unexpected property {prop} at column {column}"
                    )));
                }

                row.push(value)
            }
            if row.len() != props.len() {
                return Err(invalid(format!(
                    "state key {key} has unexpected number of properties (expected {}, found {})",
                    props.len(),
                    row.len()
                )));
            }
            rows.push(row)
        }

        Ok(Self { properties: props, rows })
    }

    pub(super) fn relevant<R: PartialEq>(&self, records: &[R]) -> Vec<bool> {
        (0..self.properties.len())
            .map(|col| {
                let values = self.rows.iter().map(|row| row[col]).collect::<HashSet<_>>().len();
                let mut groups: HashMap<Vec<&str>, Vec<&R>> = HashMap::new();
                for (row, record) in self.rows.iter().zip(records) {
                    let rest = row
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != col)
                        .map(|(_, value)| *value)
                        .collect();
                    groups.entry(rest).or_default().push(record)
                }
                !groups
                    .values()
                    .all(|group| group.len() == values && group.iter().all(|r| *r == group[0]))
            })
            .collect()
    }

    pub(super) fn keys<'b>(&'b self, relevant: &'b [bool]) -> impl Iterator<Item = String> + 'b {
        self.rows.iter().map(move |row| {
            self.properties
                .iter()
                .zip(row)
                .zip(relevant)
                .filter(|(_, is_relevant)| **is_relevant)
                .map(|((prop, value), _)| format!("{prop}={value}"))
                .collect::<Vec<_>>()
                .join(",")
        })
    }
}
