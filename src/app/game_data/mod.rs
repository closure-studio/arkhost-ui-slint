use std::collections::HashMap;

use serde::Deserialize;

#[derive(Default, Deserialize, Debug, Clone)]
#[serde(rename_all="camelCase")]
pub struct StageTable {
    pub stages: HashMap<String, Stage>
}

#[derive(Default, Deserialize, Debug, Clone)]
#[serde(rename_all="camelCase")]
pub struct Stage {
    pub name: Option<String>,
    pub code: String,
    pub ap_cost: i32,
    pub is_predefined: bool,
    pub is_hard_predefined: bool,
    pub is_skill_selectable_predefined: bool,
    pub is_story_only: bool,
    pub can_battle_replay: bool
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