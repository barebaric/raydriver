//! The Ruida packet transport and ACK handshake.
//!
//! This layer mirrors ``ruida_pa.ruidadriver.rd_transport``. USB links
//! carry raw swizzled bytes and are not acknowledged. UDP and TCP
//! links acknowledge every packet with a single byte; query packets
//! are followed by one or more replies.
//!
//! The packet state machine is explicit:
//!
//! ```text
//! Idle -> Send
//! Send -> USB: ReplyPending if query else Idle
//! Send -> UDP/TCP: AckPending
//! AckPending -> ACK: ReplyPending if query else Idle
//! AckPending -> reject byte: learn magic and retry once, else fail
//! AckPending -> timeout: Timeout
//! ReplyPending -> replies arrived: Idle
//! ReplyPending -> ACKed but no reply: Busy
//! ```

use std::time::{Duration, Instant};

use crate::core::driver::DriverError;
use crate::ruida::protocols::ruida::ruida_protocol::{
    ACK, ERR, NAK, SETTING_PREFIX,
};
use crate::ruida::rpalib::rpa_swizzler::{
    ack_magic_table, swizzle, unswizzle, unswizzle_byte,
};
use crate::ruida::ruidadriver::transport::base::{
    timed_out, Transport, TransportKind,
};
pub use crate::ruida::ruidadriver::transport::open_transport;

pub fn package(payload: &[u8], magic: u8, udp: bool) -> Vec<u8> {
    let swizzled = swizzle(payload, magic);
    if !udp {
        return swizzled;
    }
    let checksum: u16 = swizzled
        .iter()
        .fold(0u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
    let mut packet = Vec::with_capacity(swizzled.len() + 2);
    packet.push((checksum >> 8) as u8);
    packet.push((checksum & 0xFF) as u8);
    packet.extend(swizzled);
    packet
}

/// A decoded ``GET_SETTING`` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub address: u16,
    pub data: Vec<u8>,
}

/// Splits unswizzled reply bytes into replies.
///
/// A reply starts with ``0xDA`` and ends at the next byte with bit 7
/// set, because every value byte is 7-bit. Chunks shorter than nine
/// bytes are malformed and skipped; scanning continues.
pub fn unpack_replies(raw: &[u8]) -> Vec<Reply> {
    const MIN_REPLY_LEN: usize = 9;
    let mut replies = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] != 0xDA {
            break;
        }
        let mut end = index + 1;
        while end < raw.len() && raw[end] & 0x80 == 0 {
            end += 1;
        }
        let chunk = &raw[index..end];
        if chunk.len() < MIN_REPLY_LEN {
            index = end;
            continue;
        }
        if chunk[1] != 0x01 {
            break;
        }
        replies.push(Reply {
            address: (u16::from(chunk[2]) << 8) | u16::from(chunk[3]),
            data: chunk[4..].to_vec(),
        });
        index = end;
    }
    replies
}

/// How a transport frames a command packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    Raw,
    UdpChecksum,
}

/// Whether a transport acknowledges packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AckMode {
    None,
    SingleByte,
}

/// The wire behavior of one transport.
#[derive(Debug, Clone, Copy)]
struct WireProfile {
    framing: Framing,
    ack: AckMode,
    learn_magic: bool,
}

impl WireProfile {
    fn for_kind(kind: TransportKind) -> Self {
        match kind {
            TransportKind::Usb => Self {
                framing: Framing::Raw,
                ack: AckMode::None,
                learn_magic: false,
            },
            TransportKind::Udp => Self {
                framing: Framing::UdpChecksum,
                ack: AckMode::SingleByte,
                learn_magic: true,
            },
            TransportKind::Tcp => Self {
                framing: Framing::Raw,
                ack: AckMode::SingleByte,
                learn_magic: true,
            },
        }
    }

    fn frames_with_checksum(self) -> bool {
        self.framing == Framing::UdpChecksum
    }
}

/// One state of the per-packet handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketState {
    Idle,
    Send,
    AckPending,
    ReplyPending,
}

/// The result of a query packet.
#[derive(Debug)]
pub(crate) enum QueryOutcome {
    Replied(Vec<Reply>),
    /// The controller acknowledged the packet but returned no reply.
    Busy,
}

/// A link plus the Ruida packet handshake.
pub struct RdTransport {
    link: Box<dyn Transport>,
    kind: TransportKind,
    profile: WireProfile,
    magic: u8,
    ack_table: [u8; 256],
    rx: Vec<u8>,
    replies: Vec<Reply>,
    pub timeout: Duration,
    pub inter_packet_timeout: Duration,
}

impl RdTransport {
    /// Wraps a link. ``magic`` is the controller's swizzle key.
    pub fn new(
        link: Box<dyn Transport>,
        kind: TransportKind,
        magic: u8,
    ) -> Self {
        Self {
            link,
            kind,
            profile: WireProfile::for_kind(kind),
            magic,
            ack_table: ack_magic_table(),
            rx: Vec::new(),
            replies: Vec::new(),
            timeout: Duration::from_millis(500),
            inter_packet_timeout: Duration::from_millis(50),
        }
    }

    pub fn kind(&self) -> TransportKind {
        self.kind
    }

    pub fn magic(&self) -> u8 {
        self.magic
    }

    /// Sends raw (unswizzled) command bytes, chunking them into
    /// packets and acknowledging each one. No replies are expected.
    pub async fn send(&mut self, raw: &[u8]) -> Result<(), DriverError> {
        for packet in self.packets(raw) {
            self.run_packet(&packet, false).await?;
        }
        Ok(())
    }

    /// Sends one query packet and returns its replies.
    pub async fn query(
        &mut self,
        raw: &[u8],
    ) -> Result<Vec<Reply>, DriverError> {
        match self.query_outcome(raw).await? {
            QueryOutcome::Replied(replies) => Ok(replies),
            QueryOutcome::Busy => Err(DriverError::Timeout(
                "controller acknowledged the query but did not reply"
                    .to_string(),
            )),
        }
    }

    /// Sends one query packet and distinguishes Busy from failure.
    pub(crate) async fn query_outcome(
        &mut self,
        raw: &[u8],
    ) -> Result<QueryOutcome, DriverError> {
        let packet = self.pack(raw);
        match self.run_packet(&packet, true).await? {
            true => {
                Ok(QueryOutcome::Replied(std::mem::take(&mut self.replies)))
            }
            false => Ok(QueryOutcome::Busy),
        }
    }

    fn packets(&self, raw: &[u8]) -> Vec<Vec<u8>> {
        const CHUNK_SIZE: usize = 1024;
        raw.chunks(CHUNK_SIZE)
            .map(|chunk| self.pack(chunk))
            .collect()
    }

    fn pack(&self, raw: &[u8]) -> Vec<u8> {
        package(raw, self.magic, self.profile.frames_with_checksum())
    }

    /// Runs one packet through the handshake state machine.
    ///
    /// Returns ``true`` when a query produced replies and ``false``
    /// when it was acknowledged without a reply (Busy).
    async fn run_packet(
        &mut self,
        packet: &[u8],
        expects_replies: bool,
    ) -> Result<bool, DriverError> {
        let mut state = PacketState::Send;
        let mut magic_retried = false;
        loop {
            match state {
                PacketState::Idle => return Ok(false),
                PacketState::Send => {
                    self.link.write(packet).await?;
                    state = if self.profile.ack == AckMode::None {
                        if expects_replies {
                            PacketState::ReplyPending
                        } else {
                            PacketState::Idle
                        }
                    } else {
                        PacketState::AckPending
                    };
                }
                PacketState::AckPending => {
                    let Some(data) = self.read_unit(self.timeout).await? else {
                        return Err(timed_out());
                    };
                    let Some(first) = data.first().copied() else {
                        return Err(timed_out());
                    };
                    if unswizzle_byte(first, self.magic) == ACK {
                        if data.len() > 1 {
                            self.rx.extend_from_slice(&data[1..]);
                        }
                        state = if expects_replies {
                            PacketState::ReplyPending
                        } else {
                            PacketState::Idle
                        };
                        continue;
                    }
                    let code = unswizzle_byte(first, self.magic);
                    if data.len() == 1
                        && self.profile.learn_magic
                        && !magic_retried
                        && code != ERR
                        && code != NAK
                    {
                        let magic = self.ack_table[usize::from(first)];
                        if magic != 0 {
                            self.magic = magic;
                            magic_retried = true;
                            state = PacketState::Send;
                            continue;
                        }
                    }
                    if data.len() > 1 {
                        self.rx.extend_from_slice(&data[1..]);
                    }
                    let detail = match code {
                        ERR => "controller reported an error",
                        NAK => "controller reported a checksum error",
                        _ => "controller rejected the packet",
                    };
                    return Err(DriverError::Connection(format!(
                        "{detail} (0x{code:02X})"
                    )));
                }
                PacketState::ReplyPending => {
                    let count = self.collect_replies().await?;
                    return Ok(count > 0);
                }
            }
        }
    }

    /// Reads replies until no more data arrives within the
    /// inter-packet gap.
    async fn collect_replies(&mut self) -> Result<usize, DriverError> {
        let Some(mut data) = self.read_unit(self.timeout).await? else {
            return Ok(0);
        };
        loop {
            match self.link.read(self.inter_packet_timeout).await {
                Ok(Some(more)) => data.extend(more),
                Ok(None) => break,
                Err(error) => return Err(error),
            }
        }
        let raw = unswizzle(&data, self.magic);
        let replies = unpack_replies(&raw);
        let count = replies.len();
        self.replies.extend(replies);
        Ok(count)
    }

    async fn read_unit(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError> {
        if self.kind != TransportKind::Tcp {
            if !self.rx.is_empty() {
                return Ok(Some(std::mem::take(&mut self.rx)));
            }
            return self.link.read(timeout).await;
        }

        // TCP has no datagram boundaries. Re-frame the byte stream into
        // ACK/reply units: split a coalesced ACK from its replies, and
        // hold a 0xDA reply until at least the minimum frame length has
        // arrived.
        const MIN_REPLY_LEN: usize = 9;
        let deadline = Instant::now() + timeout;
        loop {
            if !self.rx.is_empty() {
                let first = unswizzle_byte(self.rx[0], self.magic);
                if first == ACK {
                    let ack = self.rx.drain(..1).collect();
                    return Ok(Some(ack));
                }
                if first == SETTING_PREFIX
                    && self.rx.len() < MIN_REPLY_LEN
                    && Instant::now() < deadline
                {
                    // Incomplete reply; fall through and read more.
                } else {
                    return Ok(Some(std::mem::take(&mut self.rx)));
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(if self.rx.is_empty() {
                    None
                } else {
                    Some(std::mem::take(&mut self.rx))
                });
            }
            let remaining = deadline.saturating_duration_since(now);
            if let Some(more) = self.link.read(remaining).await? {
                self.rx.extend(more);
            }
        }
    }

    /// Discards pending bytes and drains the underlying link.
    pub async fn drain(&mut self) {
        self.rx.clear();
        self.link.drain().await;
    }

    /// Closes the underlying link.
    pub async fn close(&mut self) {
        self.link.close().await;
        self.rx.clear();
    }
}
