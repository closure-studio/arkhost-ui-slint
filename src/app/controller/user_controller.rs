use std::sync::Arc;

use tokio::sync::oneshot;

use crate::app::utils::notification;

use super::{
    api_user_model::ApiUserModel, app_state_controller::AppStateController, sender::Sender,
    ApiOperation,
};

pub struct UserController {
    #[allow(unused)] // reserved
    api_user_model: Arc<ApiUserModel>,
    app_state_controller: Arc<AppStateController>,
    sender: Arc<Sender>,
}

impl UserController {
    pub fn new(
        api_user_model: Arc<ApiUserModel>,
        app_state_controller: Arc<AppStateController>,
        sender: Arc<Sender>,
    ) -> Self {
        Self {
            api_user_model,
            app_state_controller,
            sender,
        }
    }

    pub async fn submit_sms_verify_code(&self, code: String) {
        let (resp, mut rx) = oneshot::channel();
        let res = self
            .sender
            .send_api_request(ApiOperation::SubmitSmsVerifyCode { code, resp }, &mut rx)
            .await;
        if let Err(e) = res {
            println!("[Controller] Error submitting SMS verify code: {e}");
            notification::toast("提交验证码失败", None, &format!("{e}"), None);
        } else {
            notification::toast(
                "提交验证码成功！",
                Some("你已解锁不限时托管和更多托管槽位"),
                "",
                None,
            );
        }
    }

    pub async fn get_qq_verify_code(&self) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(ApiOperation::GetQQVerifyCode { resp }, &mut rx)
            .await
        {
            Ok(code) => {
                self.app_state_controller.exec(move |x| {
                    x.exec_in_event_loop(move |ui| {
                        let mut user_info = ui.get_user_info();
                        user_info.qq_verify_code_cached = format!("verifyCode:{code}").into();
                        ui.set_user_info(user_info);
                    })
                });
            }
            Err(e) => {
                println!("[Controller] Error fetching QQ verify code: {e}");
                notification::toast("获取QQ验证码失败", None, &format!("{e}"), None);
            }
        }
    }
}
