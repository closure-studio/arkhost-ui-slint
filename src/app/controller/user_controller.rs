use std::sync::Arc;

use super::{
    rt_api_model::RtApiModel, app_state_controller::AppStateController,
    sender::Sender,
};

pub struct UserController {
    rt_api_model: Arc<RtApiModel>,
    app_state_controller: Arc<AppStateController>,
    sender: Arc<Sender>,
}

impl UserController {
    
}
