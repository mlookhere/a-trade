use crate::{AdapterError, StreamResponse};

/// Streamer response codes documented by Schwab's market-data production contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamResponseCode {
    Success,
    LoginDenied,
    UnknownFailure,
    ServiceNotAvailable,
    CloseConnection,
    ReachedSymbolLimit,
    StreamConnectionNotFound,
    BadCommandFormat,
    FailedSubscriptions,
    FailedUnsubscriptions,
    FailedAdd,
    FailedView,
    SucceededSubscriptions,
    SucceededUnsubscriptions,
    SucceededAdd,
    SucceededView,
    StopStreaming,
    Other(i64),
}

impl From<i64> for StreamResponseCode {
    fn from(code: i64) -> Self {
        match code {
            0 => Self::Success,
            3 => Self::LoginDenied,
            9 => Self::UnknownFailure,
            11 => Self::ServiceNotAvailable,
            12 => Self::CloseConnection,
            19 => Self::ReachedSymbolLimit,
            20 => Self::StreamConnectionNotFound,
            21 => Self::BadCommandFormat,
            22 => Self::FailedSubscriptions,
            23 => Self::FailedUnsubscriptions,
            24 => Self::FailedAdd,
            25 => Self::FailedView,
            26 => Self::SucceededSubscriptions,
            27 => Self::SucceededUnsubscriptions,
            28 => Self::SucceededAdd,
            29 => Self::SucceededView,
            30 => Self::StopStreaming,
            value => Self::Other(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamResponseAction {
    Continue,
    RefreshTokenAndReconnect,
    ConnectionLimitReached,
    SymbolLimitReached,
    Reconnect,
    FailClosed,
}

#[must_use]
pub fn response_action(code: StreamResponseCode) -> StreamResponseAction {
    match code {
        StreamResponseCode::Success
        | StreamResponseCode::SucceededSubscriptions
        | StreamResponseCode::SucceededUnsubscriptions
        | StreamResponseCode::SucceededAdd
        | StreamResponseCode::SucceededView => StreamResponseAction::Continue,
        StreamResponseCode::LoginDenied => StreamResponseAction::RefreshTokenAndReconnect,
        StreamResponseCode::CloseConnection => StreamResponseAction::ConnectionLimitReached,
        StreamResponseCode::ReachedSymbolLimit => StreamResponseAction::SymbolLimitReached,
        StreamResponseCode::StreamConnectionNotFound | StreamResponseCode::StopStreaming => {
            StreamResponseAction::Reconnect
        }
        StreamResponseCode::UnknownFailure
        | StreamResponseCode::ServiceNotAvailable
        | StreamResponseCode::BadCommandFormat
        | StreamResponseCode::FailedSubscriptions
        | StreamResponseCode::FailedUnsubscriptions
        | StreamResponseCode::FailedAdd
        | StreamResponseCode::FailedView
        | StreamResponseCode::Other(_) => StreamResponseAction::FailClosed,
    }
}

pub fn require_response_success(
    response: &StreamResponse,
    expected_request_id: &str,
    expected_service: &str,
    expected_command: &str,
) -> Result<(), AdapterError> {
    if response.requestid != expected_request_id
        || response.service != expected_service
        || response.command != expected_command
    {
        return Err(AdapterError::ProviderContract(
            "stream response identity mismatch".to_owned(),
        ));
    }

    let code = StreamResponseCode::from(response.content.code);
    if response_action(code) == StreamResponseAction::Continue {
        return Ok(());
    }

    match code {
        StreamResponseCode::LoginDenied => Err(AdapterError::StreamLoginDenied),
        StreamResponseCode::CloseConnection => Err(AdapterError::StreamConnectionLimit),
        StreamResponseCode::ReachedSymbolLimit => Err(AdapterError::StreamSymbolLimit),
        _ => Err(AdapterError::Provider(format!(
            "stream command failed with code {}: {}",
            response.content.code, response.content.msg
        ))),
    }
}
