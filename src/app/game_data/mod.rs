use std::{
    cmp,
    collections::{BTreeMap, HashMap},
};

use serde::Deserialize;
use serde_repr::Deserialize_repr;

#[derive(Default, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StageTable {
    pub stages: BTreeMap<String, Stage>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StageType {
    Main,
    Daily,
    Training,
    Activity,
    Guide,
    Sub,
    Campaign,
    SpecialStory,
    HandbookBattle,
    ClimbTower,
    Other(String),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StageDiffGroup {
    None,
    Easy,
    Normal,
    Tough,
    All,
    Other(String),
}

impl StageDiffGroup {
    pub fn description(&self) -> &str {
        match self {
            StageDiffGroup::Normal => "标准",
            StageDiffGroup::Tough => "磨难",
            _ => "",
        }
    }
}

#[derive(Deserialize_repr, Debug, Clone, PartialEq, Eq)]
#[repr(i32)]
pub enum StageDropType {
    None = 0,
    Once = 1,
    Normal = 2,
    Special = 3,
    Additional = 4,
    ApReturn = 5,
    DiamondMaterial = 6,
    FurnitureDrop = 7,
    Complete = 8,
    CharmDrop = 9,
    OverrideDrop = 10,
    ItemReturn = 11,
    #[serde(other)]
    Other = i32::MIN,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StageDisplayRewards {
    pub id: String,
    pub drop_type: StageDropType,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StageDropInfo {
    pub display_rewards: Vec<StageDisplayRewards>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub name: Option<String>,
    pub stage_type: StageType,
    pub stage_drop_info: StageDropInfo,
    pub diff_group: StageDiffGroup,
    pub code: String,
    pub ap_cost: i32,
    pub is_predefined: bool,
    pub is_hard_predefined: bool,
    pub is_skill_selectable_predefined: bool,
    pub is_story_only: bool,
    pub can_battle_replay: bool,
}

impl Stage {
    pub fn display(&self) -> String {
        let mut result = String::with_capacity(16);
        if let Some(name) = &self.name {
            result.push(' ');
            result.push_str(name);
        } else {
            return result;
        }
        if !self.diff_group.description().is_empty() {
            result.push('（');
            result.push_str(self.diff_group.description());
            result.push('）');
        }
        result
    }

    pub fn code_name_components(&self) -> impl Iterator<Item = &str> {
        self.code.split('-')
    }
}

impl PartialEq for Stage {
    fn eq(&self, rhs: &Self) -> bool {
        !self.code.is_empty() && !rhs.code.is_empty() && self.code == rhs.code
    }
}

impl PartialOrd for Stage {
    fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Eq for Stage {}

impl Ord for Stage {
    fn cmp(&self, rhs: &Self) -> cmp::Ordering {
        if (self.code.is_empty() && rhs.code.is_empty()) || self.code == rhs.code {
            return cmp::Ordering::Equal;
        }

        let mut lhs_comps = self.code_name_components();
        let mut rhs_comps = rhs.code_name_components();
        loop {
            match (lhs_comps.next(), rhs_comps.next()) {
                (Some(lhs_comp), Some(rhs_comp)) => {
                    match (lhs_comp.parse::<i32>(), rhs_comp.parse::<i32>()) {
                        (Ok(lhs_num), Ok(rhs_num)) if lhs_num != rhs_num => {
                            break lhs_num.cmp(&rhs_num);
                        }
                        _ => {
                            if lhs_comp != rhs_comp {
                                break lhs_comp.cmp(rhs_comp);
                            }
                        }
                    }
                }
                (Some(_), None) => break cmp::Ordering::Greater,
                (None, Some(_)) => break cmp::Ordering::Less,
                (None, None) => break cmp::Ordering::Equal,
            }
        }
    }
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
    pub size: [f32; 2],
}
