pub mod consts;
#[cfg(feature = "desktop-app")]
/// 用于在桌面端创建WebView子进程
pub mod subprocess_webview;

use serde::{Deserialize, Serialize};
use serde_json;
use std::{collections::HashMap, rc::Rc, sync::RwLock};
use thiserror::Error;
use wry::{http::HeaderMap, PageLoadEvent, WebView, WebViewBuilder};

#[derive(Error, Debug)]
pub enum AuthenticatorError {
    #[error("webview is not assigned to this authenticator")]
    WebViewNotAssigned,
}
pub trait AuthListener {
    fn on_result(&self, result: AuthResult);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthParams {
    ArkHostAuth { user: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthAction {
    ArkHostRestrictedActionBackground {
        id: String,
        action: String,
    },
    ArkHostRestrictedActionCaptcha {
        id: String,
        action: String,
    },
    GeeTestAuth {
        id: String,
        gt: String,
        challenge: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthResult {
    Failed { id: String, err: String },
    ArkHostCaptchaTokenReCaptcha { id: String, token: String },
    ArkHostCaptchaTokenGeeTest { id: String, token: String },
    GeeTestAuth { id: String, token: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AuthPageMessage {
    ScriptInit {},
    Result { id: String, result: AuthResult },
    Log { content: String },
}

#[derive(Debug, Clone, Serialize)]
struct AuthPagePrams<'a> {
    pub params: &'a AuthParams,
    pub action: &'a AuthAction,
}

pub struct AuthResolver {
    auth_params: AuthParams,
    pending_auth: Rc<RwLock<HashMap<String, AuthAction>>>,
    webview: Rc<RwLock<WebViewStore>>,
}

impl AuthResolver {
    pub fn new(auth_params: AuthParams, webview_store: Rc<RwLock<WebViewStore>>) -> Self {
        Self {
            auth_params,
            webview: webview_store,
            pending_auth: Rc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn preform(&self, action: AuthAction) -> anyhow::Result<()> {
        let webview = self.webview.read().unwrap().webview()?;
        match action {
            AuthAction::ArkHostRestrictedActionBackground { ref id, .. }
            | AuthAction::ArkHostRestrictedActionCaptcha { ref id, .. }
            | AuthAction::GeeTestAuth { ref id, .. } => {
                self.pending_auth
                    .write()
                    .unwrap()
                    .insert(id.clone(), action.clone());

                if self.webview.read().unwrap().page_loaded {
                    self.preform_now(&webview, &action)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn settle_auth(&self, id: &str) {
        self.pending_auth.write().unwrap().remove(id);
        self.on_next_action_ready();
    }

    pub fn on_page_loaded(&self, ev: PageLoadEvent) {
        match ev {
            PageLoadEvent::Started => {}
            PageLoadEvent::Finished => {
                self.on_next_action_ready();
            }
        }
    }

    fn preform_now(&self, webview: &WebView, action: &AuthAction) -> anyhow::Result<()> {
        match action {
            AuthAction::ArkHostRestrictedActionBackground { .. }
            | AuthAction::ArkHostRestrictedActionCaptcha { .. }
            | AuthAction::GeeTestAuth { .. } => {
                let params = AuthPagePrams {
                    params: &self.auth_params,
                    action,
                };
                let params_json = serde_json::ser::to_string(&params)?;
                println!("[AuthWebView] Sent message to auth page: {}", &params_json);
                webview.evaluate_script(&[
                    "{", 
                        "let params = ", &params_json, ";", 
                        "if (window.invokeArkHostVerify) {", 
                            "setTimeout(() => window.invokeArkHostVerify(params), 0);", 
                        "} else {", 
                            "document.addEventListener('DOMContentLoaded', () => window.invokeArkHostVerify(params), false);", 
                        "}",
                    "}"].concat())?;
                Ok(())
            }
        }
    }

    fn on_next_action_ready(&self) {
        if let Err(e) = match self.webview.read().unwrap().webview() {
            Err(e) => Err(e.into()),
            Ok(webview) => {
                let pending_auth = self.pending_auth.read().unwrap();
                let mut auth = pending_auth.iter().take(1);
                match auth.next() {
                    Some((_id, action)) => self.preform_now(&webview, action),
                    None => Ok(()),
                }
            }
        } {
            println!("[AuthResolver] Error preforming action from on_next_action_ready: {e}");
        }
    }
}

pub struct Authenticator {
    auth_params: AuthParams,
    auth_listener: Rc<Box<dyn AuthListener>>,
    pub auth_resolver: Rc<AuthResolver>,
    pub webview: Rc<RwLock<WebViewStore>>,
}

impl Authenticator {
    pub fn new(auth_params: AuthParams, auth_listener: Rc<Box<dyn AuthListener>>) -> Self {
        let webview_store = Rc::new(RwLock::new(WebViewStore::new()));

        Self {
            auth_params: auth_params.clone(),
            auth_resolver: Rc::new(AuthResolver::new(auth_params, webview_store.clone())),
            auth_listener,
            webview: webview_store,
        }
    }

    pub fn build_webview<'a>(
        &self,
        builder: WebViewBuilder<'a>,
    ) -> Result<WebViewBuilder<'a>, wry::Error> {
        let mut builder = builder;

        match self.auth_params {
            AuthParams::ArkHostAuth { .. } => {
                builder = builder
                    .with_url_and_headers(consts::ARKHOST_VERIFY_URL, Self::request_headers())?;
            }
        }
        {
            let auth_listener_ref = self.auth_listener.clone();
            let auth_resolver_ref = self.auth_resolver.clone();

            builder = builder.with_ipc_handler(move |message| {
            let de_result = serde_json::de::from_str::<AuthPageMessage>(&message);
            match de_result {
                Ok(msg) => match msg {
                    AuthPageMessage::Result { result, id } => {
                        auth_resolver_ref.settle_auth(&id);
                        auth_listener_ref.on_result(result);
                    },
                    AuthPageMessage::Log { content } => println!("[AuthWebView] {content}"),
                    AuthPageMessage::ScriptInit { } => println!("[AuthWebView] Script init"),
                },
                Err(e) => {
                    println!("[AuthWebView] Error: cannot deserialize auth page message: '{message}', error: {e}")
                }
            }
        });
        }
        {
            let auth_resolver_ref = self.auth_resolver.clone();
            let webview_ref = self.webview.clone();
            builder = builder.with_on_page_load_handler(move |ev, url| {
                let state;
                match ev {
                    PageLoadEvent::Started => {
                        webview_ref.write().unwrap().page_loaded = false;
                        state = "started";
                    }
                    PageLoadEvent::Finished => {
                        webview_ref.write().unwrap().page_loaded = true;
                        state = "finished";
                    }
                }
                println!("[AuthWebView] Page load {state}; URL: {url}",);
                auth_resolver_ref.on_page_loaded(ev);
            });
        }

        Ok(builder)
    }

    pub fn reload(&self) -> anyhow::Result<()> {
        let webview = self.webview.read().unwrap().webview()?;
        webview.load_url_with_headers(consts::ARKHOST_VERIFY_URL, Authenticator::request_headers());
        Ok(())
    }

    fn request_headers() -> HeaderMap {
        HeaderMap::new()
    }
}

pub struct WebViewStore {
    pub webview: Option<Rc<WebView>>,
    pub page_loaded: bool,
}

impl WebViewStore {
    pub fn new() -> Self {
        Self {
            webview: None,
            page_loaded: false,
        }
    }

    pub fn set_webview(&mut self, webview: Rc<WebView>) {
        self.webview = Some(webview);
    }

    pub fn webview(&self) -> Result<Rc<WebView>, AuthenticatorError> {
        match &self.webview {
            Some(webview) => Ok(webview.clone()),
            None => Err(AuthenticatorError::WebViewNotAssigned),
        }
    }
}
