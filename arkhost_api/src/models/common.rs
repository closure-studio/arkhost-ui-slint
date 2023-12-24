use serde::Deserialize;
#[derive(Default, Clone, Debug)]
pub struct ResponseData<T>
where
    T: Clone,
{
    pub success: bool,
    pub data: Option<T>,
    pub internal_code: Option<i32>,
    pub internal_message: Option<String>,
}

pub trait ResponseWrapper<T>
where
    T: Clone,
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
    T: Clone,
{
    pub code: Option<i32>,
    pub data: T,
    pub message: Option<String>,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum NullableData<T>
where
    T: Clone,
{
    Data(T),
    MaybeFalse(bool),
    #[default]
    Null,
}

impl<T> ResponseWrapper<T> for ResponseWrapperNested<T>
where
    T: Clone,
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
    T: Clone,
{
    #[serde(alias = "error")] // Celebrate validation error response
    pub err: Option<String>,
    #[serde(alias = "statusCode")] // Celebrate validation error response
    pub code: Option<i32>,
    #[serde(flatten)]
    pub data: Option<T>,
}

impl<T> ResponseWrapper<T> for ResponseWrapperEmbed<T>
where
    T: Clone,
{
    fn to_response_data(self) -> ResponseData<T> {
        ResponseData {
            success: self.err == None,
            data: self.data,
            internal_code: self.code,
            internal_message: self.err,
        }
    }
}

