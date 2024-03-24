#![allow(dead_code)]
use std::path::PathBuf;

use polodb_core::bson::doc;
use serde::{Deserialize, Serialize};

use super::utils::db;

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

pub fn upsert_pending_update(release_record: &ReleaseRecord) -> polodb_core::Result<()> {
    let collection =
        db::instance().collection::<ReleaseRecord>(db::consts::collection::OTA_RELEASE);
    let mut session = db::instance().start_session()?;
    session.start_transaction(Some(polodb_core::TransactionType::Write))?;
    collection
        .delete_one_with_session(record_type_filter(RecordType::PendingUpdate), &mut session)?;
    collection.insert_one_with_session(release_record, &mut session)?;
    session.commit_transaction()
}

pub fn pending_update() -> polodb_core::Result<Option<ReleaseRecord>> {
    let collection =
        db::instance().collection::<ReleaseRecord>(db::consts::collection::OTA_RELEASE);

    collection.find_one(record_type_filter(RecordType::PendingUpdate))
}

pub fn remove_pending_update() -> polodb_core::Result<()> {
    let collection =
        db::instance().collection::<ReleaseRecord>(db::consts::collection::OTA_RELEASE);

    collection.delete_many(record_type_filter(RecordType::PendingUpdate))?;
    Ok(())
}

fn record_type_filter(rel_type: RecordType) -> polodb_core::bson::Document {
    doc! { "rel_type": { "$eq": polodb_core::bson::to_bson(&rel_type).unwrap() } }
}
