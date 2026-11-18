//! The `/api/events` stream (Server-Sent Events over a streaming fetch, since `EventSource`
//! cannot send the `Authorization` header).
//!
//! The server starts every stream with `ready`, sends `resync` when events may have been missed,
//! and a comment every 15 s as a heartbeat. Domain events are ids only; refetch what they name.

use std::collections::VecDeque;

use futures_util::{Stream, StreamExt as _, stream};

use crate::error::ApiError;
use crate::types::Event;

#[derive(Debug, Clone)]
pub enum ServerEvent {
    /// The stream is open: refetch what is on screen.
    Ready,
    /// Events may have been missed: refetch everything shown.
    Resync,
    Event(Event),
    /// An event this build does not know (a newer server): ignore it.
    Unknown {
        name: String,
    },
}

/// One dispatched SSE message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub event: String,
    pub data: String,
}

/// Incremental parser for the `text/event-stream` format (the subset the server uses: `event`,
/// `data` and comments; `id` and `retry` are ignored).
#[derive(Debug, Default)]
pub struct Parser {
    line: Vec<u8>,
    event: String,
    data: Vec<String>,
}

impl Parser {
    /// Feed bytes; returns the messages they completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for &byte in chunk {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if let Some(frame) = self.line_done(&String::from_utf8_lossy(&line)) {
                    frames.push(frame);
                }
            } else {
                self.line.push(byte);
            }
        }
        frames
    }

    fn line_done(&mut self, line: &str) -> Option<Frame> {
        if line.is_empty() {
            let event = std::mem::take(&mut self.event);
            let data = std::mem::take(&mut self.data);
            if event.is_empty() && data.is_empty() {
                return None;
            }
            return Some(Frame {
                event: if event.is_empty() {
                    "message".to_owned()
                } else {
                    event
                },
                data: data.join("\n"),
            });
        }
        if line.starts_with(':') {
            return None; // comment (heartbeat)
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => value.clone_into(&mut self.event),
            "data" => self.data.push(value.to_owned()),
            _ => {}
        }
        None
    }
}

impl From<Frame> for ServerEvent {
    fn from(frame: Frame) -> Self {
        match frame.event.as_str() {
            "ready" => Self::Ready,
            "resync" => Self::Resync,
            name => serde_json::from_str::<serde_json::Value>(&frame.data)
                .and_then(|data| {
                    serde_json::from_value(serde_json::json!({ "type": name, "data": data }))
                })
                .map_or_else(
                    |_| Self::Unknown {
                        name: name.to_owned(),
                    },
                    Self::Event,
                ),
        }
    }
}

/// The events of an open `/api/events` response, until the connection ends.
pub(crate) fn stream(
    response: reqwest::Response,
) -> impl Stream<Item = Result<ServerEvent, ApiError>> {
    let state = (
        response.bytes_stream().boxed_local(),
        Parser::default(),
        VecDeque::<ServerEvent>::new(),
    );
    stream::unfold(state, |(mut body, mut parser, mut pending)| async move {
        loop {
            if let Some(event) = pending.pop_front() {
                return Some((Ok(event), (body, parser, pending)));
            }
            match body.next().await {
                Some(Ok(chunk)) => pending.extend(parser.push(&chunk).into_iter().map(Into::into)),
                Some(Err(error)) => {
                    return Some((Err(ApiError::network(&error)), (body, parser, pending)));
                }
                None => return None,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frames_split_across_chunks() {
        let mut parser = Parser::default();
        assert_eq!(parser.push(b"event: rea"), Vec::<Frame>::new());
        assert_eq!(parser.push(b"dy\ndata: {}\n"), Vec::<Frame>::new());
        let frames =
            parser.push(b"\n: heartbeat\n\nevent: poll.created\r\ndata: {\"poll_id\":3}\r\n\r\n");
        assert_eq!(
            frames,
            vec![
                Frame {
                    event: "ready".into(),
                    data: "{}".into()
                },
                Frame {
                    event: "poll.created".into(),
                    data: r#"{"poll_id":3}"#.into()
                },
            ]
        );
    }

    #[test]
    fn joins_multiline_data_and_defaults_the_name() {
        let mut parser = Parser::default();
        let frames = parser.push(b"data: a\ndata:b\n\n");
        assert_eq!(
            frames,
            vec![Frame {
                event: "message".into(),
                data: "a\nb".into()
            }]
        );
    }

    #[test]
    fn decodes_events() {
        let event = |name: &str, data: &str| {
            ServerEvent::from(Frame {
                event: name.into(),
                data: data.into(),
            })
        };
        assert!(matches!(event("ready", "{}"), ServerEvent::Ready));
        assert!(matches!(event("resync", "{}"), ServerEvent::Resync));
        assert!(matches!(
            event("poll.reaction", r#"{"poll_id":1,"user_id":2,"emoji":"🔥"}"#),
            ServerEvent::Event(Event::PollReaction {
                poll_id: 1,
                user_id: 2,
                ..
            })
        ));
        assert!(matches!(
            event(
                "session.revoked",
                r#"{"session_id":"8f9c1f0e-2b8e-4a4e-9a51-6a3f0c0d1e2f"}"#
            ),
            ServerEvent::Event(Event::SessionRevoked { .. })
        ));
        assert!(
            matches!(event("future.thing", "{}"), ServerEvent::Unknown { name } if name == "future.thing")
        );
        assert!(matches!(
            event("poll.created", "not json"),
            ServerEvent::Unknown { .. }
        ));
    }
}
