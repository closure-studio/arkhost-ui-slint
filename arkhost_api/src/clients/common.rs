use crate::models::common::{ResponseData, ResponseWrapper};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json;
use std::fmt::Debug;
use thiserror::Error;

pub type ApiResult<T> = anyhow::Result<T>;

#[derive(Error, Debug)]
pub enum UnauthorizedError {
    #[error("登陆已失效")]
    MissingUserCredentials,
    #[error("用户已被封禁")]
    BannedUser,
}

#[derive(Error, Debug)]
#[error("请求错误\n- 状态码: {status_code}{}{}{}{}{}",
    .internal_status_code.as_ref().map_or("".into(), |x| format!("\n- 内部代码: {x}")),
    .internal_message.as_ref().map_or("".into(), |x| format!("\n- 信息: {x}")),
    .raw_data.as_ref().map_or("".into(), |x| format!("\n- 原始数据:\n{x}")),
    .raw_response.as_ref().map_or("".into(), |x| format!("\n- 原始数据:\n{x:?}")),
    .source_error.as_ref().map_or("".into(), |x| format!("\n- 错误源（若非网络问题等外部因素，请提交Bug）\n{:?}", *x))
    )]
pub struct ResponseError<TRes>
where
    TRes: Debug,
{
    pub status_code: u16,
    pub raw_data: Option<String>,
    pub raw_response: Option<TRes>,
    pub internal_status_code: Option<i32>,
    pub internal_message: Option<String>,
    pub source_error: Option<anyhow::Error>,
}

pub async fn try_response_json<T>(response: reqwest::Response) -> anyhow::Result<T>
where
    T: 'static + Send + Sync + Debug + for<'de> Deserialize<'de>,
{
    let status_code = response.status();
    let json_str = response.text().await?;
    match serde_json::de::from_str::<T>(&json_str) {
        Ok(data) => Ok(data),
        Err(serde_err) => Err(ResponseError::<T> {
            status_code: status_code.as_u16(),
            internal_status_code: None,
            internal_message: None,
            raw_response: None,
            raw_data: Some(json_str),
            source_error: Some(serde_err.into()),
        }
        .into()),
    }
}

pub fn try_response_data<T>(
    status_code: StatusCode,
    resp: impl ResponseWrapper<T>,
) -> Result<T, ResponseError<ResponseData<T>>>
where
    T: Clone + Debug,
{
    let data = resp.to_response_data();
    match data {
        ResponseData {
            success: true,
            data: Some(data),
            ..
        } => Ok(data),
       _ => Err(ResponseError {
            status_code: status_code.as_u16(),
            internal_status_code: data.internal_code.clone(),
            internal_message: data.internal_message.clone(),
            raw_response: Some(data),
            raw_data: None,
            source_error: None,
        }),
    }
}

pub trait UserState: Debug + Send + Sync {
    fn set_login_state(&mut self, jwt: String);
    fn get_login_state(&self) -> Option<String>;
    fn erase_login_state(&mut self);
}

#[derive(Debug)]
pub struct UserStateMemStorage {
    jwt: Option<String>,
}

impl UserStateMemStorage {
    pub fn new(jwt: Option<String>) -> Self {
        Self { jwt }
    }
}

impl UserState for UserStateMemStorage {
    fn set_login_state(&mut self, jwt: String) {
        self.jwt = Some(jwt);
    }

    fn get_login_state(&self) -> Option<String> {
        return self.jwt.clone();
    }

    fn erase_login_state(&mut self) {
        self.jwt = None;
    }
}
