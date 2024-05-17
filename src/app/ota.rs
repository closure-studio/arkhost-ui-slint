#![allow(dead_code)]
use super::utils::db;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordType {
    PendingUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Blob {
    File(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub blob: Blob,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseRecord {
    pub rel_type: RecordType,
    pub version: semver::Version,
    pub binary: Resource,
}

pub fn upsert_pending_update(release_record: &ReleaseRecord) -> heed::Result<()> {
    let env = db::env();
    let db = db()?;
    let mut wtxn = env.write_txn()?;
    db.put(
        &mut wtxn,
        &record_type_key(RecordType::PendingUpdate)?,
        release_record,
    )?;
    wtxn.commit()
}

pub fn pending_update() -> heed::Result<Option<ReleaseRecord>> {
    let env = db::env();
    let db = db()?;
    let rtxn = env.read_txn()?;

    db.get(&rtxn, &record_type_key(RecordType::PendingUpdate)?)
}

pub fn remove_pending_update() -> heed::Result<()> {
    let env = db::env();
    let db = db()?;
    let mut wtxn = env.write_txn()?;
    db.delete(&mut wtxn, &record_type_key(RecordType::PendingUpdate)?)?;
    wtxn.commit()
}

fn db() -> heed::Result<heed::Database<heed::types::Str, heed::types::SerdeBincode<ReleaseRecord>>>
{
    db::database(Some(db::consts::db::OTA_RELEASE))
}

fn record_type_key(rel_type: RecordType) -> heed::Result<String> {
    serde_json::ser::to_string(&rel_type).map_err(|e| heed::Error::Encoding(e.into()))
}
