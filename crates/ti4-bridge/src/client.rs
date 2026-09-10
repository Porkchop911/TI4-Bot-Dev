//! Talking to the bridge endpoint from Rust.
//!
//! The endpoint itself is `bridge/server.py`, vendored verbatim from the historical repository and
//! deliberately not ported: it is standard-library only, binds `127.0.0.1`, imports nothing from
//! the old engine, and has been exercised by thousands of real games. Rewriting it in Rust could
//! only have made it worse, and it satisfies what M11-002 through M11-005 asked for.
//!
//! So this is a *client*, not a server, which is a far smaller surface. It speaks the minimum of
//! HTTP/1.1 that the endpoint speaks back:
//!
//! - one request per connection, `Connection: close`, so there is no keep-alive state to get wrong;
//! - `Content-Length` framing only — the endpoint always sends it and never chunks;
//! - loopback only unless the caller insists, because the endpoint binds loopback and a bridge
//!   reachable from the network is a table anyone can move pieces on.
//!
//! No HTTP crate, on purpose. Adding a dependency to a shared workspace mid-experiment is a way to
//! break somebody else's build for no benefit; this is eighty lines against a server we ship.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::hexsummary::{self, Board};
use crate::wire::{
    Command, CommandBatch, Health, LogResponse, PollRequest, QueueResponse, TurnResponse, Upload,
};

/// Where the mod posts, and therefore where the endpoint listens.
///
/// The port is compiled into the TTS mod and is not configurable from the table's side, so a
/// different one here would silently receive nothing.
pub const DEFAULT_ADDRESS: &str = "127.0.0.1:8080";

/// How long any single exchange may take before the bridge is treated as unreachable.
///
/// The executor polls every two seconds; a client that blocks for longer than that is worse than
/// one that fails and is retried.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The most a response may be. A board summary is a few kilobytes; a megabyte is already absurd,
/// and the cap is what stops a wrong or hostile `Content-Length` from asking for all of memory.
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

/// Why an exchange with the bridge did not produce an answer.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The bridge could not be reached, or the exchange did not finish in time.
    #[error("bridge at {address} unreachable: {source}")]
    Unreachable {
        address: String,
        #[source]
        source: std::io::Error,
    },
    /// The address named a host that is not loopback.
    #[error("{0} is not a loopback address; pass allow_remote to mean it")]
    NotLoopback(String),
    /// The reply was not HTTP this client can read.
    #[error("bridge replied with something this client cannot read: {0}")]
    Malformed(String),
    /// The bridge answered, and said no.
    #[error("bridge refused with HTTP {status}: {body}")]
    Refused { status: u16, body: String },
    /// The reply was JSON, but not the shape this endpoint promises.
    #[error("bridge reply was not the expected shape: {0}")]
    Unexpected(#[from] serde_json::Error),
    /// The board on the table did not decode.
    #[error(transparent)]
    Board(#[from] hexsummary::MalformedSummary),
    /// No upload has arrived yet, so there is no board to read.
    #[error("no upload has arrived yet")]
    NoUpload,
}

/// A connection-per-request client for one bridge endpoint.
#[derive(Debug, Clone)]
pub struct BridgeClient {
    address: String,
    allow_remote: bool,
}

impl Default for BridgeClient {
    fn default() -> Self {
        Self::new(DEFAULT_ADDRESS)
    }
}

impl BridgeClient {
    /// A client for the endpoint at `address`.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            allow_remote: false,
        }
    }

    /// Permit a non-loopback address.
    ///
    /// Separate and explicit because the endpoint binds loopback on purpose: it has no
    /// authentication of any kind, and anything that can reach it can move pieces on the table.
    #[must_use]
    pub fn allowing_remote(mut self) -> Self {
        self.allow_remote = true;
        self
    }

    /// The address this client talks to.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Is the bridge up, and what does it say it can do?
    ///
    /// # Errors
    /// [`ClientError`] if the bridge is unreachable or answers something unreadable.
    pub fn health(&self) -> Result<Health, ClientError> {
        self.get_json("/")
    }

    /// The most recent upload from the table.
    ///
    /// # Errors
    /// [`ClientError::NoUpload`] before the mod has posted anything, and [`ClientError`] otherwise.
    pub fn latest(&self) -> Result<serde_json::Map<String, serde_json::Value>, ClientError> {
        match self.request("GET", "/latest", None) {
            Ok(body) => Ok(serde_json::from_str(&body)?),
            // The endpoint answers 404 for "nothing yet", which is a state rather than a fault.
            Err(ClientError::Refused { status: 404, .. }) => Err(ClientError::NoUpload),
            Err(error) => Err(error),
        }
    }

    /// The board on the table, decoded.
    ///
    /// The whole point of the bridge in one call: the mod's summary, read into a typed board.
    ///
    /// # Errors
    /// [`ClientError::NoUpload`] before anything has arrived, [`ClientError::Board`] if the summary
    /// does not parse, and [`ClientError`] for transport faults.
    pub fn board(&self) -> Result<Board, ClientError> {
        let payload = self.latest()?;
        let summary = payload
            .get("hexSummary")
            .and_then(serde_json::Value::as_str)
            .ok_or(ClientError::NoUpload)?;
        Ok(hexsummary::decode(summary)?)
    }

    /// Everything the executor has reported since the bridge started.
    ///
    /// # Errors
    /// [`ClientError`] if the bridge is unreachable or answers something unreadable.
    pub fn log(&self) -> Result<Vec<String>, ClientError> {
        let response: LogResponse = self.get_json("/log")?;
        Ok(response.log)
    }

    /// Whose turn the executor last reported, and when.
    ///
    /// `None` before any executor poll has carried one.
    ///
    /// # Errors
    /// [`ClientError`] if the bridge is unreachable or answers something unreadable.
    pub fn turn(&self) -> Result<Option<TurnResponse>, ClientError> {
        match self.request("GET", "/turn", None) {
            Ok(body) => Ok(Some(serde_json::from_str(&body)?)),
            Err(ClientError::Refused { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Queue one command for the table, and learn the id it was given.
    ///
    /// # Errors
    /// [`ClientError::Refused`] if the bridge rejects the command, and [`ClientError`] otherwise.
    pub fn queue(&self, command: &Command) -> Result<QueueResponse, ClientError> {
        let body = serde_json::to_string(command)?;
        let response = self.request("POST", "/queue", Some(&body))?;
        Ok(serde_json::from_str(&response)?)
    }

    /// Poll as the executor does: hand over log lines and collect whatever is waiting.
    ///
    /// Intended for tests and for driving a table with no TTS attached. The real executor polls on
    /// its own timer; a second poller competes with it for the same queue.
    ///
    /// # Errors
    /// [`ClientError`] if the bridge is unreachable or answers something unreadable.
    pub fn poll(&self, request: &PollRequest) -> Result<CommandBatch, ClientError> {
        let body = serde_json::to_string(request)?;
        let response = self.request("POST", "/poll", Some(&body))?;
        Ok(serde_json::from_str(&response)?)
    }

    /// Post telemetry as the mod does.
    ///
    /// Intended for tests: it is how a board can be put in front of the bridge with no table.
    ///
    /// # Errors
    /// [`ClientError`] if the bridge is unreachable or answers something unreadable.
    pub fn upload(
        &self,
        payload: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<CommandBatch, ClientError> {
        let body = serde_json::to_string(payload)?;
        let response = self.request("POST", "/data", Some(&body))?;
        Ok(serde_json::from_str(&response)?)
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let body = self.request("GET", path, None)?;
        Ok(serde_json::from_str(&body)?)
    }

    /// One request, one connection, one answer.
    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, ClientError> {
        self.check_loopback()?;
        let mut stream = self.connect()?;

        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            self.address
        );
        if let Some(body) = body {
            head.push_str("Content-Type: application/json\r\n");
            head.push_str("Content-Length: ");
            head.push_str(&body.len().to_string());
            head.push_str("\r\n");
        }
        head.push_str("\r\n");

        let unreachable = |source: std::io::Error| ClientError::Unreachable {
            address: self.address.clone(),
            source,
        };
        stream.write_all(head.as_bytes()).map_err(unreachable)?;
        if let Some(body) = body {
            stream.write_all(body.as_bytes()).map_err(unreachable)?;
        }
        stream.flush().map_err(unreachable)?;

        // `Connection: close` means end-of-stream is end-of-body, so the whole reply can be read
        // without interpreting Content-Length at all -- but the cap still applies, because a
        // server that never closes would otherwise read forever.
        let mut raw = Vec::new();
        stream
            .take(MAX_RESPONSE as u64 + 1)
            .read_to_end(&mut raw)
            .map_err(unreachable)?;
        if raw.len() > MAX_RESPONSE {
            return Err(ClientError::Malformed(format!(
                "reply exceeded {MAX_RESPONSE} bytes"
            )));
        }

        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text
            .split_once("\r\n\r\n")
            .ok_or_else(|| ClientError::Malformed("no header/body break".to_owned()))?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| ClientError::Malformed(format!("no status line in {head:?}")))?;

        if !(200..300).contains(&status) {
            return Err(ClientError::Refused {
                status,
                body: body.trim().to_owned(),
            });
        }
        Ok(body.to_owned())
    }

    fn connect(&self) -> Result<TcpStream, ClientError> {
        let unreachable = |source: std::io::Error| ClientError::Unreachable {
            address: self.address.clone(),
            source,
        };
        let target = self
            .address
            .to_socket_addrs()
            .map_err(unreachable)?
            .next()
            .ok_or_else(|| {
                ClientError::Malformed(format!("{} resolved to no address", self.address))
            })?;
        let stream = TcpStream::connect_timeout(&target, TIMEOUT).map_err(unreachable)?;
        stream
            .set_read_timeout(Some(TIMEOUT))
            .map_err(unreachable)?;
        stream
            .set_write_timeout(Some(TIMEOUT))
            .map_err(unreachable)?;
        Ok(stream)
    }

    fn check_loopback(&self) -> Result<(), ClientError> {
        if self.allow_remote {
            return Ok(());
        }
        let resolved = self
            .address
            .to_socket_addrs()
            .map_err(|source| ClientError::Unreachable {
                address: self.address.clone(),
                source,
            })?
            .next()
            .ok_or_else(|| {
                ClientError::Malformed(format!("{} resolved to no address", self.address))
            })?;
        if resolved.ip().is_loopback() {
            Ok(())
        } else {
            Err(ClientError::NotLoopback(self.address.clone()))
        }
    }
}

/// The `Upload` record the endpoint keeps, rebuilt from a `/latest` payload.
///
/// `/latest` serves the payload alone, not the record around it, so the path and arrival time are
/// not recoverable from it. This exists so a caller holding a payload can still use [`Upload`]'s
/// accessors rather than reaching into the map by hand.
#[must_use]
pub fn upload_from_payload(payload: serde_json::Map<String, serde_json::Value>) -> Upload {
    Upload {
        path: "/latest".to_owned(),
        args: std::collections::BTreeMap::new(),
        payload,
        received_at: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;

    /// A one-shot server that records the request it was given and replies with `reply`.
    ///
    /// Real sockets rather than a trait, because the thing worth testing here is the HTTP framing
    /// this module writes and reads, and a mock of my own framing would prove nothing.
    fn serve_once(reply: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let address = listener.local_addr().expect("bound").to_string();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one connection");
            let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            // Read a body if the request announced one, so a POST's payload can be asserted on.
            let length = request
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length: ")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if length > 0 {
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).expect("the announced body");
                request.push_str("\r\n");
                request.push_str(&String::from_utf8_lossy(&body));
            }
            let _ = sender.send(request);
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        });
        (address, receiver)
    }

    fn ok_reply(body: &'static str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn a_health_check_reads_the_features_back() {
        let reply: &'static str = Box::leak(
            ok_reply(r#"{"ok":true,"uploads":2,"pending":0,"features":["latest","log"]}"#)
                .into_boxed_str(),
        );
        let (address, requests) = serve_once(reply);
        let health = BridgeClient::new(address).health().expect("a health check");
        assert!(health.ok);
        assert_eq!(health.uploads, 2);
        assert_eq!(health.features, vec!["latest".to_owned(), "log".to_owned()]);

        let request = requests.recv().expect("the request");
        assert!(request.starts_with("GET / HTTP/1.1\r\n"), "{request:?}");
        assert!(
            request.contains("Connection: close\r\n"),
            "one request per connection, or the read never ends: {request:?}"
        );
    }

    #[test]
    fn queueing_a_command_sends_it_as_the_body_and_reads_the_id_back() {
        let reply: &'static str = Box::leak(
            ok_reply(r#"{"queued":{"id":9,"action":"ping"},"id":9,"pending":1}"#).into_boxed_str(),
        );
        let (address, requests) = serve_once(reply);
        let queued = BridgeClient::new(address)
            .queue(&Command::new("ping"))
            .expect("a queued command");
        assert_eq!(queued.id, crate::wire::CommandId(9));
        assert_eq!(queued.queued.action, "ping");

        let request = requests.recv().expect("the request");
        assert!(
            request.starts_with("POST /queue HTTP/1.1\r\n"),
            "{request:?}"
        );
        assert!(request.contains("Content-Length: 17\r\n"), "{request:?}");
        assert!(request.ends_with(r#"{"action":"ping"}"#), "{request:?}");
    }

    #[test]
    fn nothing_uploaded_yet_is_a_state_and_not_a_fault() {
        // The endpoint answers 404 before the mod has posted anything. A caller polling an empty
        // table must be able to tell that from a bridge that has fallen over.
        let reply: &'static str = "HTTP/1.1 404 Not Found\r\nContent-Length: 38\r\n\r\n\
                                   {\"error\": \"no upload has arrived yet\"}";
        let (address, _requests) = serve_once(reply);
        let error = BridgeClient::new(address).board().expect_err("nothing yet");
        assert!(matches!(error, ClientError::NoUpload), "{error}");
    }

    #[test]
    fn a_board_arrives_decoded() {
        let reply: &'static str =
            Box::leak(ok_reply(r#"{"hexSummary":"18+0+0Bc;Bi","round":2}"#).into_boxed_str());
        let (address, _requests) = serve_once(reply);
        let board = BridgeClient::new(address).board().expect("a board");
        assert_eq!(board.tiles(), vec!["18"]);
        assert_eq!(
            board.system("18").expect("tile 18").planets[0].occupiers(),
            vec![hexsummary::Colour::Blue]
        );
    }

    #[test]
    fn a_refusal_carries_its_status_and_its_reason() {
        let reply: &'static str = "HTTP/1.1 400 Bad Request\r\nContent-Length: 44\r\n\r\n\
                                   {\"error\": \"a command needs an 'action'\"}";
        let (address, _requests) = serve_once(reply);
        let error = BridgeClient::new(address)
            .queue(&Command::new("ping"))
            .expect_err("refused");
        match error {
            ClientError::Refused { status, body } => {
                assert_eq!(status, 400);
                assert!(body.contains("action"), "{body}");
            }
            other => panic!("expected a refusal, got {other}"),
        }
    }

    #[test]
    fn a_bridge_that_is_not_there_is_unreachable_rather_than_a_hang() {
        // Port 1 on loopback: nothing listens, and the connect fails immediately.
        let error = BridgeClient::new("127.0.0.1:1")
            .health()
            .expect_err("nothing listens on port 1");
        assert!(matches!(error, ClientError::Unreachable { .. }), "{error}");
    }

    #[test]
    fn a_non_loopback_address_is_refused_unless_the_caller_insists() {
        // The endpoint has no authentication of any kind. Reaching one across a network is either
        // a mistake or somebody else's table.
        let error = BridgeClient::new("93.184.216.34:8080")
            .health()
            .expect_err("not loopback");
        assert!(matches!(error, ClientError::NotLoopback(_)), "{error}");

        // And the escape hatch exists, so a tunnelled bridge is possible for someone who means it.
        let insisting = BridgeClient::new("93.184.216.34:8080").allowing_remote();
        assert!(!matches!(
            insisting.health(),
            Err(ClientError::NotLoopback(_))
        ));
    }

    #[test]
    fn a_reply_that_is_not_http_is_refused_rather_than_guessed_at() {
        let (address, _requests) = serve_once("this is not a response");
        let error = BridgeClient::new(address).health().expect_err("not HTTP");
        assert!(matches!(error, ClientError::Malformed(_)), "{error}");
    }
}
