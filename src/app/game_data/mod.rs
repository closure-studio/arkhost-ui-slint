use std::collections::HashMap;

use serde::Deserialize;

pub type StageTable = HashMap<String, Stage>;

#[derive(Default, Deserialize, Debug, Clone)]
pub struct Stage {
    pub name: String,
    pub code: String,
    pub ap: u16,
    pub items: Vec<String>
}

pub type CharPackSummaryTable = HashMap<String, CharPack>;

#[derive(Default, Deserialize, Debug, Clone)]
pub struct CharPack {
    pub name: String,
    #[serde(rename = "pv")]
    pub pivot_factor: [f32; 2],
    #[serde(rename = "of")]
    pub pivot_offset: [f32; 2],
    #[serde(rename = "sc")]
    pub scale: [f32; 2],
    #[serde(rename = "sz")]
    pub size: [f32; 2]
}