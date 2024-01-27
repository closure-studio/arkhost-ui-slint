use serde::Deserialize;
use std::fmt::Debug;

#[derive(Default, Clone, Debug)]
pub struct ResponseData<T>
where
    T: Clone + Debug + Default,
{
    pub success: bool,
    pub data: Option<T>,
    pub internal_code: Option<i32>,
    pub internal_message: Option<String>,
}

pub trait ResponseWrapper<T>
where
    T: Clone + Debug + Default,
{
    fn to_response_data(self) -> ResponseData<T>;
}

/// 嵌套响应数据包装对象，形如
/// ```json
/// {
///     code: 1,
///     data: { "foo": 42 }，
///     message: "成功"
/// }
/// ```
#[derive(Default, Deserialize, Clone, Debug)]
pub struct ResponseWrapperNested<T>
where
    T: Clone + Debug + Default,
{
    pub code: Option<i32>,
    pub data: T,
    pub message: Option<String>,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum NullableData<T>
where
    T: Clone + Debug + Default,
{
    Data(T),
    MaybeFalse(bool),
    #[default]
    Null,
}

impl<T> ResponseWrapper<T> for ResponseWrapperNested<T>
where
    T: Clone + Debug + Default,
{
    fn to_response_data(self) -> ResponseData<T> {
        ResponseData {
            success: self.code == Some(1),
            data: Some(self.data),
            internal_code: self.code,
            internal_message: self.message,
        }
    }
}

#[derive(Default, Deserialize, Clone, Debug)]
pub struct ErrorInfoEmbed {
    #[serde(alias = "error")] // Celebrate validation error response
    pub err: Option<String>,
    #[serde(alias = "statusCode")] // Celebrate validation error response
    pub code: Option<i32>,
}

/// 内嵌响应数据包装对象，形如
/// ```json
/// {
///     "err": "错误",
///     "code": 400,
///     "bar": "baz"
/// }
/// ```
#[derive(Default, Deserialize, Clone, Debug)]
pub struct ResponseWrapperEmbed<T>
where
    T: Clone + Debug + Default,
{
    #[serde(flatten)]
    pub error: ErrorInfoEmbed,
    #[serde(flatten)]
    pub data: Option<T>,
}

impl<T> ResponseWrapper<T> for ResponseWrapperEmbed<T>
where
    T: Clone + Debug + Default,
{
    fn to_response_data(self) -> ResponseData<T> {
        ResponseData {
            success: self.error.err.is_none(),
            data: self.data,
            internal_code: self.error.code,
            internal_message: self.error.err,
        }
    }
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ResponseWrapperEmbedUnion<T>
where
    T: Clone + Debug + Default,
{
    Data(T),
    Err(ErrorInfoEmbed),
    #[default]
    None,
}

impl<T> ResponseWrapper<T> for ResponseWrapperEmbedUnion<T>
where
    T: Clone + Debug + Default,
{
    fn to_response_data(self) -> ResponseData<T> {
        match self {
            ResponseWrapperEmbedUnion::Data(data) => ResponseData {
                success: true,
                data: Some(data),
                ..ResponseData::default()
            },
            ResponseWrapperEmbedUnion::Err(ErrorInfoEmbed { err, code }) => ResponseData {
                success: false,
                data: None,
                internal_code: code,
                internal_message: err,
            },
            ResponseWrapperEmbedUnion::None => ResponseData {
                success: false,
                data: None,
                ..ResponseData::default()
            },
        }
    }
}
