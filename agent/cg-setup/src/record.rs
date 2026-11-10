//! `setup-attempt.json`, the record of one preflight (SPEC-024 Amendment
//! 2026-11-10, item 5): what it created or is about to create, the anchor's
//! fingerprints and whether the product service was running. It names no
//! path and holds no secret, so a record cannot direct a deletion elsewhere:
//! each name it holds stands for one fixed path under the data root's parent.

use std::collections::BTreeSet;

use cg_agent::service::anchor::is_fingerprint;
use serde::Deserialize;
use serde_json::{Map, Value};

/// The record's name in the data root.
pub const RECORD: &str = "setup-attempt.json";

/// `format`'s one value.
pub const FORMAT: u64 = 1;

/// The most a record may hold.
pub const RECORD_LIMIT: usize = 64 * 1024;

/// A name the record's `created` may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Created {
    ProductRoot,
    DataRoot,
    StagedAnchor,
    StagedConfig,
}

impl Created {
    /// Every name, in the order a record lists them.
    pub const ALL: [Created; 4] = [
        Created::ProductRoot,
        Created::DataRoot,
        Created::StagedAnchor,
        Created::StagedConfig,
    ];

    /// The name as the record writes it.
    pub fn key(self) -> &'static str {
        match self {
            Created::ProductRoot => "product_root",
            Created::DataRoot => "data_root",
            Created::StagedAnchor => "staged_anchor",
            Created::StagedConfig => "staged_config",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Created::ALL.into_iter().find(|name| name.key() == key)
    }
}

/// The record's keys, each exactly once.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Keys {
    format: Value,
    created: Value,
    fingerprints: Value,
    service_running: Value,
}

/// One preflight's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The names it created or is about to create.
    pub created: BTreeSet<Created>,
    /// The anchor's fingerprints; none when nothing is staged.
    pub fingerprints: BTreeSet<String>,
    /// Whether the product service was in a state other than `STOPPED`
    /// before preflight stopped it.
    pub service_running: bool,
}

impl Record {
    /// The record as JSON: exactly its four keys.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut object = Map::new();
        object.insert("format".into(), Value::from(FORMAT));
        object.insert(
            "created".into(),
            Value::Array(
                self.created
                    .iter()
                    .map(|name| Value::from(name.key()))
                    .collect(),
            ),
        );
        object.insert(
            "fingerprints".into(),
            Value::Array(
                self.fingerprints
                    .iter()
                    .map(|f| Value::from(f.as_str()))
                    .collect(),
            ),
        );
        object.insert("service_running".into(), Value::Bool(self.service_running));
        // A map of strings, numbers, arrays and a boolean always serializes.
        serde_json::to_vec(&Value::Object(object)).expect("a JSON value")
    }

    /// The record of `bytes`, if they have its form: a JSON object of at
    /// most 64 KiB with exactly its four keys, `format` the number 1,
    /// `created` an array of its names, `fingerprints` an array of
    /// fingerprints and `service_running` a boolean. The error is the reason,
    /// which quotes no value of the file.
    pub fn parse(bytes: &[u8]) -> Result<Record, String> {
        if bytes.len() > RECORD_LIMIT {
            return Err(format!("it is larger than {RECORD_LIMIT} bytes"));
        }
        let value: Value =
            serde_json::from_slice(bytes).map_err(|_| "it is not JSON".to_string())?;
        if !value.is_object() {
            return Err("it is not a JSON object".to_string());
        }
        // A key missing, another key or a key given twice: the derived
        // reader refuses each, where a map keeps the last of two.
        let keys: Keys = serde_json::from_slice(bytes).map_err(|_| {
            "its keys are not exactly format, created, fingerprints and service_running, each once"
                .to_string()
        })?;
        // `1.0` is a float, which `as_u64` does not read.
        if keys.format.as_u64() != Some(FORMAT) {
            return Err(format!("its format is not {FORMAT}"));
        }
        let strings = |value: &'_ Value, key: &str| -> Result<Vec<String>, String> {
            value
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .map(|item| item.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                })
                .ok_or_else(|| format!("its {key} is not an array of strings"))
        };
        let created = strings(&keys.created, "created")?
            .into_iter()
            .map(|key| {
                Created::from_key(&key)
                    .ok_or_else(|| "its created holds a name that is not the setup's".to_string())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let fingerprints = strings(&keys.fingerprints, "fingerprints")?
            .into_iter()
            .map(|value| {
                if is_fingerprint(&value) {
                    Ok(value)
                } else {
                    Err("its fingerprints hold a value that is not a fingerprint".to_string())
                }
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let Some(service_running) = keys.service_running.as_bool() else {
            return Err("its service_running is not a boolean".to_string());
        };
        Ok(Record {
            created,
            fingerprints,
            service_running,
        })
    }
}
