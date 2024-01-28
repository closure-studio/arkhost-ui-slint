use std::sync::{atomic::Ordering, Arc};

use anyhow::anyhow;
use arkhost_api::clients::common::ResponseError;
use arkhost_api::models::api_quota::{
    SlotRuleValidationResult, UpdateSlotAccountRequest, UpdateSlotAccountResponse,
};
use tokio::sync::oneshot;

use crate::app::app_state::mapping::{SlotInfoMapping, UserInfoMapping};
use crate::app::controller::AuthCommand;
use crate::app::ui::UserIdApiRequestState;

use super::AuthResult;
use super::{
    app_state_controller::AppStateController, request_controller::RequestController,
    rt_api_model::RtApiModel, ApiOperation,
};

pub struct SlotController {
    rt_api_model: Arc<RtApiModel>,
    app_state_controller: Arc<AppStateController>,
    request_controller: Arc<RequestController>,
}

impl SlotController {
    pub fn new(
        rt_api_model: Arc<RtApiModel>,
        app_state_controller: Arc<AppStateController>,
        request_controller: Arc<RequestController>,
    ) -> Self {
        Self {
            rt_api_model,
            app_state_controller,
            request_controller,
        }
    }

    pub async fn refresh_slots_by_rt_model(&self) {
        let slot_map = self.rt_api_model.user.slots.read().await;
        let mut slot_list = vec![];
        for (uuid, slot_ref) in slot_map.iter() {
            let order = slot_ref.order.load(Ordering::Acquire);
            let slot_entry = slot_ref.slot.read().await;
            slot_list.push((
                order,
                uuid.clone(),
                SlotInfoMapping::from(slot_entry.clone()),
            ));
        }
        self.app_state_controller
            .exec(move |x| x.update_slot_info_list(slot_list));
    }

    pub async fn refresh_slots(&self) {
        self.app_state_controller
            .exec(|x| x.set_user_id_api_request_state(UserIdApiRequestState::Requesting));

        let (resp, mut rx) = oneshot::channel();
        if let Ok(user_state_data) = self
            .request_controller
            .send_api_request(ApiOperation::GetUserStateData { resp }, &mut rx)
            .await
        {
            let (resp, mut rx) = oneshot::channel();
            match self
                .request_controller
                .send_api_request(ApiOperation::GetRegistryUserInfo { resp }, &mut rx)
                .await
            {
                Ok(user) => {
                    let initial_slot_added = user.slots.iter().any(|x| x.game_account.is_some());
                    let user_info_mapping = UserInfoMapping {
                        nickname: user_state_data.account,
                        status: user.id_server_status,
                        phone: user.id_server_phone,
                        qq: user.id_server_qq,
                        sms_verify_slot_added: initial_slot_added,
                    };

                    self.app_state_controller
                        .exec(move |x| x.update_user_info(user_info_mapping));
                    self.rt_api_model
                        .user
                        .handle_retrieve_slots_result(user.slots)
                        .await;
                    self.refresh_slots_by_rt_model().await;
                }
                Err(e) => {
                    println!("[Controller] Error retrieving Registry API user {e}");
                }
            }
        }

        self.app_state_controller
            .exec(|x| x.set_user_id_api_request_state(UserIdApiRequestState::Idle));
    }

    pub async fn update_slot(&self, id: String, update_request: UpdateSlotAccountRequest) {
        self.app_state_controller.exec(|x| {
            x.set_slot_update_request_state(
                id.clone(),
                crate::SlotUpdateRequestState::Requesting,
                None,
            )
        });
        let (resp1, rx1) = oneshot::channel();
        let (resp2, rx2) = oneshot::channel();
        let mut auth_attempts = [
            (
                AuthCommand::AuthArkHostBackground {
                    resp: resp1,
                    action: "slot".into(),
                },
                rx1,
            ),
            (
                AuthCommand::AuthArkHostCaptcha {
                    resp: resp2,
                    action: "slot".into(),
                },
                rx2,
            ),
        ]
        .into_iter();
        let mut update_result = anyhow::Result::Err(anyhow!(""));
        while let Some((auth_command, mut rx_auth_controller)) = auth_attempts.next() {
            match &update_result {
                Ok(_) => break,
                Err(e) => println!("[Controller] failed attempting to update slot {id}: {e:?}"),
            }

            update_result = self
                .try_update_slot(
                    auth_command,
                    id.clone(),
                    update_request.clone(),
                    &mut rx_auth_controller,
                )
                .await;
        }
        _ = self
            .request_controller
            .tx_auth_controller
            .send(AuthCommand::HideWindow {})
            .await;

        match update_result {
            Ok(result) => {
                let mut available = None;
                if let Some(validation_result) = result.data {
                    available = Some(validation_result.available);
                    let slot_ref = self
                        .rt_api_model
                        .user
                        .record_slot_verify_result(&id, update_request, validation_result)
                        .await;
                    if let Some(slot_ref) = slot_ref {
                        let mapping = SlotInfoMapping::from(slot_ref.slot.read().await.clone());
                        let id = id.clone();
                        self.app_state_controller
                            .exec(move |x| x.update_slot_info(id, mapping));
                    }
                }
                let available = available.unwrap_or(result.success);

                let status_text = if available {
                    "".to_owned()
                } else {
                    let e = ResponseError::<SlotRuleValidationResult> {
                        status_code: 0,
                        internal_status_code: result.internal_code,
                        internal_message: result.internal_message,
                        ..ResponseError::default()
                    };

                    format!("更新失败\n{e}")
                };
                self.app_state_controller.exec(move |x| {
                    x.set_slot_update_request_state(
                        id,
                        if available {
                            crate::SlotUpdateRequestState::Success
                        } else {
                            crate::SlotUpdateRequestState::Fail
                        },
                        Some(status_text),
                    )
                });
            }
            Err(e) => {
                let error_info = format!("更新失败\n{e}");
                self.app_state_controller.exec(move |x| {
                    x.set_slot_update_request_state(
                        id.clone(),
                        crate::SlotUpdateRequestState::Fail,
                        Some(error_info),
                    )
                });
            }
        }
        
        self.refresh_slots().await;
    }

    async fn try_update_slot(
        &self,
        auth_command: AuthCommand,
        id: String,
        update_request: UpdateSlotAccountRequest,
        rx_auth_controller: &mut oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<UpdateSlotAccountResponse> {
        let auth_result = self
            .request_controller
            .send_auth_request(auth_command, rx_auth_controller)
            .await?;
        let captcha_token = match auth_result {
            AuthResult::ArkHostCaptchaTokenReCaptcha { token, .. } => token,
            AuthResult::ArkHostCaptchaTokenGeeTest { token, .. } => token,
            _ => {
                return Err(anyhow!("unexpected auth result: {auth_result:?}"));
            }
        };

        let (resp, mut rx) = oneshot::channel();
        let result = self
            .request_controller
            .send_api_request(
                ApiOperation::UpdateSlotAccount {
                    slot_uuid: id,
                    captcha_token,
                    request: update_request,
                    resp,
                },
                &mut rx,
            )
            .await?;
        match &result.internal_code {
            Some(arkhost_api::consts::quota::error_code::CAPTCHA_ERROR) => {
                Err(anyhow!("captcha failed"))
            }
            _ => Ok(result),
        }
    }
}
