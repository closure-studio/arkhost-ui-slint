use anyhow::anyhow;
use arkhost_api::models::api_quota::{SlotRuleValidationResult, UpdateSlotAccountRequest};
use arkhost_api::models::{api_arkhost, api_quota};
use async_trait::async_trait;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::DerefMut;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type GameRef = Arc<InnerGameRef>;
pub type GameMap = HashMap<String, GameRef>;
pub type GameMapSync = RwLock<GameMap>;

#[async_trait]
impl OrderedMapEntry<api_arkhost::GameInfo> for GameRef {
    fn order(&self) -> &AtomicI32 {
        &self.order
    }

    async fn update(&self, new_val: api_arkhost::GameInfo) -> () {
        self.game.write().await.info = new_val;
    }
}

pub type SlotRef = Arc<InnerSlotRef>;
pub type SlotMap = HashMap<String, SlotRef>;
pub type SlotMapSync = RwLock<SlotMap>;

#[async_trait]
impl OrderedMapEntry<api_quota::Slot> for SlotRef {
    fn order(&self) -> &AtomicI32 {
        &self.order
    }

    async fn update(&self, new_val: api_quota::Slot) -> () {
        self.slot.write().await.data = new_val;
    }
}

#[derive(Debug)]
pub struct InnerGameRef {
    pub order: AtomicI32,
    pub game: RwLock<GameEntry>,
}

#[derive(Debug, Clone)]
pub struct GameEntry {
    pub info: api_arkhost::GameInfo,
    pub details: Option<api_arkhost::GameDetails>,
    pub logs: Vec<api_arkhost::LogEntry>,
    pub log_cursor_back: u64,
    pub log_cursor_front: u64,
    pub stage_name: Option<String>,
}

impl GameEntry {
    pub fn new(info: api_arkhost::GameInfo) -> Self {
        Self {
            info,
            details: None,
            logs: Vec::new(),
            log_cursor_back: 0,
            log_cursor_front: 0,
            stage_name: None,
        }
    }
}

#[derive(Debug)]
pub struct InnerSlotRef {
    pub order: AtomicI32,
    pub slot: RwLock<SlotEntry>,
}

#[derive(Debug, Clone)]
pub struct SlotEntry {
    pub data: api_quota::Slot,
    pub sync_state: SlotSyncState,
    pub last_update_request: Option<UpdateSlotAccountRequest>,
    pub last_update_response: Option<SlotRuleValidationResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSyncState {
    Unknown,
    Synchronized,
    Pending,
}

impl SlotEntry {
    pub fn new(data: api_quota::Slot) -> Self {
        Self {
            data,
            sync_state: SlotSyncState::Unknown,
            last_update_request: None,
            last_update_response: None,
        }
    }
}

#[derive(Debug)]
pub struct RtUserModel {
    pub games: GameMapSync,
    pub slots: SlotMapSync,
    initial_games_fetched: AtomicBool,
}

#[async_trait]
trait OrderedMapEntry<T> {
    fn order(&self) -> &AtomicI32;
    async fn update(&self, new_val: T) -> ();
}

impl RtUserModel {
    pub fn new() -> Self {
        Self {
            games: RwLock::new(HashMap::new()),
            slots: RwLock::new(HashMap::new()),
            initial_games_fetched: false.into(),
        }
    }

    pub async fn clear(&self) {
        self.games.write().await.clear();
        self.slots.write().await.clear();
    }

    pub async fn handle_retrieve_games_result(&self, games: Vec<api_arkhost::GameInfo>) {
        {
            let mut game_map = self.games.write().await;

            self.update_ordered_map_entries(
                game_map.deref_mut(),
                games,
                |x| &x.status.account,
                |i, x| {
                    Arc::new(InnerGameRef {
                        order: (i as i32).into(),
                        game: RwLock::new(GameEntry::new(x)),
                    })
                },
            )
            .await;
        }
        self.initial_games_fetched.store(true, Ordering::Release);
    }

    pub async fn handle_retrieve_slots_result(&self, mut slots: Vec<api_quota::Slot>) {
        slots.sort_by_key(|x| -x.user_tier_availability_rank());
        {
            let mut slot_map = self.slots.write().await;

            self.update_ordered_map_entries(
                slot_map.deref_mut(),
                slots,
                |x| &x.uuid,
                |i, x| {
                    Arc::new(InnerSlotRef {
                        order: (i as i32).into(),
                        slot: RwLock::new(SlotEntry::new(x)),
                    })
                },
            )
            .await;
        }
    }

    pub async fn update_slot_sync_state(&self) -> bool {
        if !self.initial_games_fetched.load(Ordering::Acquire) {
            return false;
        }

        let mut modified = false;

        let game_map = self.games.read().await;
        for (_, v) in self.slots.read().await.iter() {
            let mut slot_entry = v.slot.write().await;
            let pervious_state = slot_entry.sync_state;
            slot_entry.sync_state = match &slot_entry.data.game_account {
                None => SlotSyncState::Synchronized,
                Some(game) if game_map.contains_key(game) => SlotSyncState::Synchronized,
                _ => SlotSyncState::Pending,
            };
            modified = modified || (slot_entry.sync_state != pervious_state);
        }

        modified
    }

    pub async fn record_slot_verify_result(
        &self,
        id: &str,
        request: UpdateSlotAccountRequest,
        response: SlotRuleValidationResult,
    ) -> Option<SlotRef> {
        let slot_map = self.slots.read().await;
        if let Some(slot_ref) = slot_map.get(id) {
            let mut slot_entry = slot_ref.slot.write().await;
            slot_entry.last_update_request = Some(request);
            slot_entry.last_update_response = Some(response);
            Some(slot_ref.clone())
        } else {
            None
        }
    }

    async fn update_ordered_map_entries<K, T, U>(
        &self,
        map: &mut HashMap<K, U>,
        items: Vec<T>,
        get_key_fn: impl Fn(&T) -> &K,
        new_entry_fn: impl Fn(usize, T) -> U,
    ) where
        K: Clone + Eq + Hash,
        U: OrderedMapEntry<T>,
    {
        let items_len = items.len();
        for (i, item) in items.into_iter().enumerate() {
            let key = get_key_fn(&item);
            if let Some(existing_item) = map.get_mut(key) {
                existing_item.update(item).await;
                existing_item.order().store(i as i32, Ordering::Release);
            } else {
                map.retain(|_, v| v.order().load(Ordering::Acquire) != i as i32);
                map.insert(key.clone(), new_entry_fn(i, item));
            }
        }
        map.retain(|_, v| v.order().load(Ordering::Acquire) < items_len as i32);
    }

    pub async fn find_game(&self, account: &str) -> anyhow::Result<GameRef> {
        match self.games.read().await.get(account) {
            None => Err(anyhow!(format!("game with account '{account}' not found"))),
            Some(game) => Ok(game.clone()),
        }
    }
}
