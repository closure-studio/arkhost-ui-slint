use std::sync::Arc;

use super::{
    rt_api_model::RtApiModel, app_state_controller::AppStateController,
    request_controller::RequestController,
};

pub struct UserController {
    rt_api_model: Arc<RtApiModel>,
    app_state_controller: Arc<AppStateController>,
    request_controller: Arc<RequestController>,
}

impl UserController {
    
}
