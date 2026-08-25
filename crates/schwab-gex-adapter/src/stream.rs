use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::Message,
};

use crate::{AdapterError, StreamerInfo};

const OPTION_FIELDS: &str = "0,9,12,13,20,21,22,23,26,29,38";
const EQUITY_FIELDS: &str = "0,33,34";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamService {
    Admin,
    LevelOneOptions,
    LevelOneEquities,
}

impl StreamService {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "ADMIN",
            Self::LevelOneOptions => "LEVELONE_OPTIONS",
            Self::LevelOneEquities => "LEVELONE_EQUITIES",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCommand {
    Login,
    Subs,
    Add,
    Unsubs,
    Logout,
}

impl StreamCommand {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "LOGIN",
            Self::Subs => "SUBS",
            Self::Add => "ADD",
            Self::Unsubs => "UNSUBS",
            Self::Logout => "LOGOUT",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StreamDataBatch {
    pub service: String,
    pub timestamp: i64,
    pub command: String,
    #[serde(default)]
    pub content: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StreamResponse {
    pub service: String,
    pub command: String,
    pub requestid: String,
    #[serde(rename = "SchwabClientCorrelId")]
    pub schwab_client_correl_id: String,
    pub timestamp: i64,
    pub content: StreamResponseContent,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StreamResponseContent {
    pub code: i64,
    pub msg: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Responses(Vec<StreamResponse>),
    Data(Vec<StreamDataBatch>),
    Notify(Value),
    Closed,
}

#[derive(Serialize)]
struct RequestEnvelope<'a> {
    requests: Vec<StreamRequest<'a>>,
}

#[derive(Serialize)]
struct StreamRequest<'a> {
    requestid: String,
    service: &'a str,
    command: &'a str,
    #[serde(rename = "SchwabClientCustomerId")]
    customer_id: &'a str,
    #[serde(rename = "SchwabClientCorrelId")]
    correl_id: &'a str,
    parameters: Value,
}

#[derive(Debug, Clone)]
pub struct StreamRequestFactory {
    customer_id: String,
    correl_id: String,
    channel: String,
    function_id: String,
    next_request_id: u64,
}

impl StreamRequestFactory {
    pub fn new(info: &StreamerInfo) -> Result<Self, AdapterError> {
        for (name, value) in [
            ("stream customer id", info.schwab_client_customer_id.as_str()),
            ("stream correl id", info.schwab_client_correl_id.as_str()),
            ("stream channel", info.schwab_client_channel.as_str()),
            ("stream function id", info.schwab_client_function_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AdapterError::InvalidInput(name));
            }
        }
        Ok(Self {
            customer_id: info.schwab_client_customer_id.clone(),
            correl_id: info.schwab_client_correl_id.clone(),
            channel: info.schwab_client_channel.clone(),
            function_id: info.schwab_client_function_id.clone(),
            next_request_id: 0,
        })
    }

    pub fn login(&mut self, access_token: &str) -> Result<(String, String), AdapterError> {
        if access_token.trim().is_empty() {
            return Err(AdapterError::InvalidInput("access token"));
        }
        let request_id = self.take_request_id();
        let parameters = serde_json::json!({
            "Authorization": access_token,
            "SchwabClientChannel": self.channel,
            "SchwabClientFunctionId": self.function_id,
        });
        Ok((request_id.clone(), self.encode(&request_id, StreamService::Admin, StreamCommand::Login, parameters)?))
    }

    pub fn subscribe_options(
        &mut self,
        option_symbols: &[String],
        command: StreamCommand,
    ) -> Result<(String, String), AdapterError> {
        if option_symbols.is_empty() {
            return Err(AdapterError::InvalidInput("option symbols"));
        }
        if !matches!(command, StreamCommand::Subs | StreamCommand::Add | StreamCommand::Unsubs) {
            return Err(AdapterError::InvalidInput("option stream command"));
        }
        let keys = join_keys(option_symbols)?;
        let request_id = self.take_request_id();
        let parameters = if command == StreamCommand::Unsubs {
            serde_json::json!({"keys": keys})
        } else {
            serde_json::json!({"keys": keys, "fields": OPTION_FIELDS})
        };
        Ok((
            request_id.clone(),
            self.encode(
                &request_id,
                StreamService::LevelOneOptions,
                command,
                parameters,
            )?,
        ))
    }

    pub fn subscribe_underlyings(
        &mut self,
        underlyings: &[String],
        command: StreamCommand,
    ) -> Result<(String, String), AdapterError> {
        if underlyings.is_empty() {
            return Err(AdapterError::InvalidInput("underlyings"));
        }
        if !matches!(command, StreamCommand::Subs | StreamCommand::Add | StreamCommand::Unsubs) {
            return Err(AdapterError::InvalidInput("underlying stream command"));
        }
        let keys = join_keys(underlyings)?;
        let request_id = self.take_request_id();
        let parameters = if command == StreamCommand::Unsubs {
            serde_json::json!({"keys": keys})
        } else {
            serde_json::json!({"keys": keys, "fields": EQUITY_FIELDS})
        };
        Ok((
            request_id.clone(),
            self.encode(
                &request_id,
                StreamService::LevelOneEquities,
                command,
                parameters,
            )?,
        ))
    }

    pub fn logout(&mut self) -> Result<(String, String), AdapterError> {
        let request_id = self.take_request_id();
        Ok((
            request_id.clone(),
            self.encode(
                &request_id,
                StreamService::Admin,
                StreamCommand::Logout,
                serde_json::json!({}),
            )?,
        ))
    }

    fn take_request_id(&mut self) -> String {
        let request_id = self.next_request_id.to_string();
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    fn encode(
        &self,
        request_id: &str,
        service: StreamService,
        command: StreamCommand,
        parameters: Value,
    ) -> Result<String, AdapterError> {
        serde_json::to_string(&RequestEnvelope {
            requests: vec![StreamRequest {
                requestid: request_id.to_owned(),
                service: service.as_str(),
                command: command.as_str(),
                customer_id: &self.customer_id,
                correl_id: &self.correl_id,
                parameters,
            }],
        })
        .map_err(|error| AdapterError::ProviderContract(error.to_string()))
    }
}

fn join_keys(keys: &[String]) -> Result<String, AdapterError> {
    if keys.iter().any(|key| key.trim().is_empty() || key.contains(',')) {
        return Err(AdapterError::InvalidInput("stream key"));
    }
    Ok(keys.join(","))
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct SchwabStreamClient {
    socket: Socket,
}

impl SchwabStreamClient {
    pub async fn connect(streamer_socket_url: &str) -> Result<Self, AdapterError> {
        let parsed = url::Url::parse(streamer_socket_url)
            .map_err(|error| AdapterError::ProviderContract(error.to_string()))?;
        if parsed.scheme() != "wss" {
            return Err(AdapterError::ProviderContract(
                "streamer socket must use wss".to_owned(),
            ));
        }
        let (socket, _) = connect_async(streamer_socket_url)
            .await
            .map_err(|error| AdapterError::Transport(error.to_string()))?;
        Ok(Self { socket })
    }

    pub async fn send_json(&mut self, json: String) -> Result<(), AdapterError> {
        self.socket
            .send(Message::Text(json.into()))
            .await
            .map_err(|error| AdapterError::Transport(error.to_string()))
    }

    pub async fn next_event(&mut self) -> Result<StreamEvent, AdapterError> {
        loop {
            let Some(message) = self.socket.next().await else {
                return Ok(StreamEvent::Closed);
            };
            let message = message.map_err(|error| AdapterError::Transport(error.to_string()))?;
            match message {
                Message::Text(text) => return parse_event(text.as_ref()),
                Message::Binary(bytes) => {
                    let text = std::str::from_utf8(&bytes)
                        .map_err(|error| AdapterError::ProviderContract(error.to_string()))?;
                    return parse_event(text);
                }
                Message::Ping(payload) => {
                    self.socket
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| AdapterError::Transport(error.to_string()))?;
                }
                Message::Pong(_) => {}
                Message::Close(_) => return Ok(StreamEvent::Closed),
                Message::Frame(_) => {}
            }
        }
    }
}

fn parse_event(text: &str) -> Result<StreamEvent, AdapterError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| AdapterError::ProviderContract(error.to_string()))?;
    if let Some(responses) = value.get("response") {
        return serde_json::from_value(responses.clone())
            .map(StreamEvent::Responses)
            .map_err(|error| AdapterError::ProviderContract(error.to_string()));
    }
    if let Some(data) = value.get("data") {
        return serde_json::from_value(data.clone())
            .map(StreamEvent::Data)
            .map_err(|error| AdapterError::ProviderContract(error.to_string()));
    }
    if let Some(notify) = value.get("notify") {
        return Ok(StreamEvent::Notify(notify.clone()));
    }
    Err(AdapterError::ProviderContract(
        "stream message omitted response/data/notify".to_owned(),
    ))
}
