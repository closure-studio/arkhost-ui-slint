use crate::models::{
    api_passport::UserStateData,
    common::{ResponseData, ResponseWrapper},
};
use base64::Engine;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json;
use std::fmt::Debug;
use thiserror::Error;

pub type ApiResult<T> = anyhow::Result<T>;

#[derive(Debug)]
pub struct ResponseErrorInfo {
    pub status_code: u16,
    pub internal_status_code: Option<i32>,
    pub internal_message: Option<String>,
}

pub trait ResponseErrorInfoSource {
    fn error_info(&self) -> ResponseErrorInfo;
}

#[derive(Error, Debug)]
pub enum UnauthorizedError {
    #[error("登录已失效")]
    MissingUserCredentials,
    #[error("用户已被封禁")]
    BannedUser,
}

#[derive(Default, Error, Debug)]
#[error("请求错误{}\n- 状态码: {status_code}{}{}{}{}",
    .internal_message.as_ref().map_or("".into(), |x| format!("：{x}")),
    .internal_status_code.as_ref().map_or("".into(), |x| format!("\n- 内部代码: {x}")),
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

impl<TRes> ResponseErrorInfoSource for ResponseError<TRes>
where
    TRes: Debug,
{
    fn error_info(&self) -> ResponseErrorInfo {
        ResponseErrorInfo {
            status_code: self.status_code,
            internal_status_code: self.internal_status_code,
            internal_message: self.internal_message.clone(),
        }
    }
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
) -> Result<T, ResponseError<T>>
where
    T: Clone + Debug + Default,
{
    map_try_response_data(status_code, resp, |x| Ok(x))
}

pub fn map_try_response_data<T, R>(
    status_code: StatusCode,
    resp: impl ResponseWrapper<T>,
    op: impl FnOnce(T) -> Result<R, T>,
) -> Result<R, ResponseError<T>>
where
    T: Clone + Debug + Default,
{
    fn make_err<T: Clone + Debug>(
        status_code: StatusCode,
        data: Option<T>,
        internal_code: Option<i32>,
        internal_message: Option<String>,
    ) -> ResponseError<T> {
        ResponseError {
            status_code: status_code.as_u16(),
            internal_status_code: internal_code,
            internal_message,
            raw_response: data,
            raw_data: None,
            source_error: None,
        }
    }

    let data = resp.to_response_data();
    match data {
        ResponseData {
            success: true,
            data: Some(data),
            internal_code,
            internal_message,
        } => match op(data) {
            Ok(r) => Ok(r),
            Err(data) => Err(make_err(
                status_code,
                Some(data),
                internal_code,
                internal_message,
            )),
        },
        ResponseData {
            data,
            internal_code,
            internal_message,
            ..
        } => Err(make_err(status_code, data, internal_code, internal_message)),
    }
}

pub trait UserState: Debug + Send + Sync {
    fn set_login_state(&mut self, jwt: String);
    fn login_state(&self) -> Option<String>;
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

    fn login_state(&self) -> Option<String> {
        self.jwt.clone()
    }

    fn erase_login_state(&mut self) {
        self.jwt = None;
    }
}

pub trait UserStateDataSource {
    fn user_state_data(&self) -> Option<UserStateData>;
}

impl<T: UserState + ?Sized> UserStateDataSource for T {
    fn user_state_data(&self) -> Option<UserStateData> {
        if let Some(token) = self.login_state() {
            let mut iter = token.splitn(3, '.');
            if let (Some(_), Some(payload), Some(_), None) =
                (iter.next(), iter.next(), iter.next(), iter.next())
            {
                if let Ok(state) = base64::engine::general_purpose::STANDARD_NO_PAD
                    .decode(payload)
                    .map_err(anyhow::Error::from)
                    .and_then(|x| {
                        serde_json::de::from_slice::<UserStateData>(&x).map_err(anyhow::Error::from)
                    })
                {
                    return Some(state);
                }
            }
        }

        None
    }
}

pub fn headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "X-Platform",
        reqwest::header::HeaderValue::from_static(crate::consts::CLIENT_IDENTIFIER),
    );
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(crate::consts::CLIENT_USER_AGENT),
    );

    headers
}

pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::ClientBuilder::new()
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .max_tls_version(reqwest::tls::Version::TLS_1_3)
        .http1_only()
}
