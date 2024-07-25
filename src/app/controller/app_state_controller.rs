use crate::app::app_state::{AppState, AppStateAsyncOp};
use std::sync::{Arc, Mutex, MutexGuard};

pub struct AppStateController {
    pub app_state: Arc<Mutex<AppState>>,
}

impl AppStateController {
    pub fn app_state(&self) -> MutexGuard<'_, AppState> {
        self.app_state.lock().unwrap()
    }

    pub fn exec(&self, func: impl FnOnce(&AppState) -> AppStateAsyncOp) {
        let op = func(&self.app_state());
        op.exec();
    }

    pub async fn exec_wait(&self, func: impl FnOnce(&AppState) -> AppStateAsyncOp) {
        let op = func(&self.app_state());
        op.exec_wait().await;
    }
}
