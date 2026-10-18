//! Single-owner private Unix command exchange. The host supervisor supplies the
//! original socket, peer/executable custody and fresh authenticated channel.
//! This module never connects, launches, reconnects, retries or grants admission.
use harbor_llm_protocol::{
    Binding, ErrorCode, ProtocolError,
    commands::{self, AuthenticatedChannel, Command, CommandBody},
};
use std::{
    io::{ErrorKind, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::OnceLock,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, ProtocolError>;
fn reject(code: ErrorCode) -> ProtocolError {
    ProtocolError { code }
}
fn io_error(_: std::io::Error) -> ProtocolError {
    reject(ErrorCode::TransportFailed)
}

struct Ceilings {
    record: usize,
    overhead: usize,
    records: u32,
    timeout: Duration,
}
fn ceilings() -> &'static Ceilings {
    static CEILINGS: OnceLock<Ceilings> = OnceLock::new();
    CEILINGS.get_or_init(|| {
        let command: serde_json::Value =
            serde_json::from_str(commands::SCHEMA).expect("checked-in command policy");
        let channel: serde_json::Value = serde_json::from_str(harbor_llm_protocol::channel::SCHEMA)
            .expect("checked-in channel policy");
        let transport: serde_json::Value =
            serde_json::from_str(harbor_llm_protocol::TRANSPORT_POLICY)
                .expect("checked-in transport policy");
        let overhead = (channel["sequenceBytes"].as_u64().unwrap()
            + channel["lengthBytes"].as_u64().unwrap()
            + channel["tagBytes"].as_u64().unwrap()) as usize;
        Ceilings {
            record: command["maxFrameBytes"].as_u64().unwrap() as usize + overhead,
            overhead,
            // ponytail: one configuration and one input transfer plus three
            // commands/receipts; multi-turn sessions need a new explicit budget.
            records: command["maxChunks"].as_u64().unwrap() as u32 * 2 + 10,
            timeout: Duration::from_millis(transport["maxTimeoutMs"].as_u64().unwrap()),
        }
    })
}

/// Required whole-channel limits. Counts include both directions; bytes include
/// each outer u32 length and authenticated record. Consumers may only tighten.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Total elapsed time from construction, never refreshed by partial progress.
    pub timeout: Duration,
    /// Total incoming and outgoing records, including transfers and receipts.
    pub records: u32,
    /// Total incoming and outgoing wire bytes, not just decoded content.
    pub bytes: u64,
}

/// One original socket/channel for one binding. Not cloneable. Every failure
/// shuts down the socket and zeroizes retained keys; no partial reply escapes.
/// Caller-protected endpoint custody is required for the entire lifetime. Do not
/// retain socket clones or share its descriptor with backend/daemon writers.
pub struct Exchange {
    stream: UnixStream,
    channel: AuthenticatedChannel,
    binding: Binding,
    limits: Limits,
    deadline: Instant,
    records: u32,
    bytes: u64,
    closed: bool,
}
impl Exchange {
    /// Take ownership of a supervisor-established private channel. Construction
    /// performs no peer I/O. Channel credentials must be fresh and bound to this
    /// exact registration by the launcher; strings alone are not peer authority.
    pub fn new(
        stream: UnixStream,
        channel: AuthenticatedChannel,
        binding: Binding,
        limits: Limits,
    ) -> Result<Self> {
        let ceiling = ceilings();
        if limits.timeout.is_zero()
            || limits.timeout > ceiling.timeout
            || limits.records == 0
            || limits.records > ceiling.records
            || limits.bytes == 0
            || limits.bytes > u64::from(ceiling.records) * (ceiling.record as u64 + 4)
        {
            return Err(reject(ErrorCode::BudgetExhausted));
        }
        commands::encode(&Command {
            schema_version: 2,
            command_schema_digest: commands::SCHEMA_DIGEST.into(),
            request_id: "ipc-binding".into(),
            binding: binding.clone(),
            body: CommandBody::Reopen(commands::Target {
                handle: binding.execution_id.clone(),
            }),
        })?;
        stream.set_nonblocking(false).map_err(io_error)?;
        Ok(Self {
            stream,
            channel,
            binding,
            limits,
            deadline: Instant::now() + limits.timeout,
            records: 0,
            bytes: 0,
            closed: false,
        })
    }
    fn remaining(&self) -> Result<Duration> {
        if self.closed {
            return Err(reject(ErrorCode::TransportFailed));
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(reject(ErrorCode::BudgetExhausted));
        }
        Ok(remaining)
    }
    fn count_record(&mut self) -> Result<()> {
        self.remaining()?;
        if self.records >= self.limits.records {
            return Err(reject(ErrorCode::BudgetExhausted));
        }
        self.records += 1;
        Ok(())
    }
    fn count_bytes(&mut self, count: usize) -> Result<()> {
        if count as u64 > self.limits.bytes - self.bytes {
            return Err(reject(ErrorCode::BudgetExhausted));
        }
        self.bytes += count as u64;
        Ok(())
    }
    fn write_all(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            self.stream
                .set_write_timeout(Some(self.remaining()?))
                .map_err(io_error)?;
            match self.stream.write(bytes) {
                Ok(0) => return Err(reject(ErrorCode::TransportFailed)),
                Ok(count) => bytes = &bytes[count..],
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.remaining()?;
                    return Err(reject(ErrorCode::TransportFailed));
                }
            }
        }
        self.remaining()?;
        Ok(())
    }
    fn read_exact(&mut self, mut bytes: &mut [u8]) -> Result<()> {
        while !bytes.is_empty() {
            self.stream
                .set_read_timeout(Some(self.remaining()?))
                .map_err(io_error)?;
            match self.stream.read(bytes) {
                Ok(0) => return Err(reject(ErrorCode::TransportFailed)),
                Ok(count) => bytes = &mut bytes[count..],
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.remaining()?;
                    return Err(reject(ErrorCode::TransportFailed));
                }
            }
        }
        self.remaining()?;
        Ok(())
    }
    fn send(&mut self, command: &Command) -> Result<()> {
        self.remaining()?;
        if command.binding != self.binding {
            return Err(reject(ErrorCode::AuthorityDenied));
        }
        let bytes = self.channel.seal(command)?;
        self.count_record()?;
        self.count_bytes(4 + bytes.len())?;
        self.write_all(&(bytes.len() as u32).to_be_bytes())?;
        self.write_all(&bytes)
    }
    fn receive(&mut self, request: &Command) -> Result<Command> {
        self.count_record()?;
        self.count_bytes(4)?;
        let mut header = [0; 4];
        self.read_exact(&mut header)?;
        let length = u32::from_be_bytes(header) as usize;
        if length <= ceilings().overhead || length > ceilings().record {
            return Err(reject(ErrorCode::InvalidFrame));
        }
        self.count_bytes(length)?;
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes)?;
        let reply = self.channel.open(&bytes)?;
        if reply.binding != self.binding {
            return Err(reject(ErrorCode::AuthorityDenied));
        }
        if reply.request_id != request.request_id {
            return Err(reject(ErrorCode::TransportFailed));
        }
        Ok(reply)
    }
    /// Send one transfer record without a response. Effectful commands cannot use
    /// this path. The dispatcher checks transfer sequence, purpose and contents.
    pub fn send_transfer(&mut self, command: &Command) -> Result<()> {
        let result = (|| {
            self.remaining()?;
            if !matches!(
                command.body,
                CommandBody::TransferBegin(_)
                    | CommandBody::TransferChunk(_)
                    | CommandBody::TransferEnd(_)
            ) {
                return Err(reject(ErrorCode::UnsupportedCapability));
            }
            self.send(command)
        })();
        if result.is_err() {
            self.close();
        }
        result
    }
    /// Exchange exactly one initial command/reply. Invoke only inside the
    /// original controller's `dispatch_initial` callback, after its fsynced
    /// reservation. Reply meaning remains the controller/application's policy.
    pub fn request(&mut self, command: &Command) -> Result<Command> {
        let result = (|| {
            self.remaining()?;
            if !matches!(
                command.body,
                CommandBody::Create(_) | CommandBody::Submit(_) | CommandBody::Execute(_)
            ) {
                return Err(reject(ErrorCode::UnsupportedCapability));
            }
            self.send(command)?;
            self.receive(command)
        })();
        if result.is_err() {
            self.close();
        }
        result
    }
    /// Terminal socket shutdown and key disposal. This does not revoke provider
    /// authority, stop a supervised process or establish descendant cleanup.
    pub fn close(&mut self) {
        self.closed = true;
        self.channel.close();
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}
impl Drop for Exchange {
    fn drop(&mut self) {
        self.close();
    }
}
