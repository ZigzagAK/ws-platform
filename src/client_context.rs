/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::ops::{ Deref, DerefMut };
use std::net::SocketAddr;
use std::io::ErrorKind;
use std::time::Duration;
use mio::{ Events, Interest, Poll, Token };

use crate::connection_pool::StreamType;
use crate::buffer::Buffer;
use crate::error::{ CoreError, Code, Code::* };
use crate::core::State;

pub struct ClientContext {
    stream: StreamType,
    pub (crate) inner: Option<State>,
    pub server_addr: SocketAddr,
    pub buf: Buffer
}

impl Deref for ClientContext {
    type Target = StreamType;
    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for ClientContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}

impl ClientContext {
    pub fn new(stream: StreamType, server_addr: SocketAddr) -> ClientContext {
        ClientContext {
            server_addr: server_addr,
            inner: None,
            stream: stream,
            buf: Buffer::default()
        }
    }

    pub (crate) fn with_state(stream: StreamType, server_addr: SocketAddr, state: State) -> ClientContext {
        ClientContext {
            server_addr: server_addr,
            inner: Some(state),
            stream: stream,
            buf: Buffer::default()
        }
    }

    #[allow(dead_code)]
    fn poll(&mut self, i: Interest, timeout: Option<Duration>) -> std::io::Result<Code> {
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1);

        poll.registry().register(&mut self.stream, Token(0), i)?;

        if let Err(err) = poll.poll(&mut events, timeout) {
            return match err.kind() {
                ErrorKind::TimedOut => Ok(AGAIN),
                ErrorKind::Interrupted => Ok(DECLINED),
                _ => Err(err)
            }
        }

        Ok(OK)
    }

    pub fn read(&mut self) -> Result<Code, CoreError> {
        self.buf.reset();
        loop {
            match self.buf.read(&mut self.stream) {
                Ok((true, _)) => {
                    /* eof */
                    return Ok(DECLINED);
                },
                Ok(_) => {
                    return Ok(OK);
                },
                Err(err) => {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        ErrorKind::TimedOut => {
                            return throw!("Timeout while waiting for data from client")
                        },
                        ErrorKind::WouldBlock => return Ok(AGAIN),
                        _ => {
                            return throw!("Failed to receive data from client: {}", err);
                        }
                    }
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.buf.reset()
    }

    pub fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes())
    }

    pub fn write(&mut self, buf: &[u8]) {
        self.buf.extend(buf)
    }

    pub fn flush(&mut self) -> Result<(Code, usize), CoreError> {
        let mut sent = 0;
        loop {
            match self.buf.write(&mut self.stream) {
                Ok((false, sz)) => {
                    return Ok((AGAIN, sent + sz));
                },
                Ok((true, sz)) => {
                    sent += sz;
                    return Ok((OK, sent));
                },
                Err(err) => {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        ErrorKind::TimedOut => {
                            return throw!("Timeout while sending data to client")
                        },
                        ErrorKind::WouldBlock => return Ok((AGAIN, sent)),
                        _ => {
                            return throw!("Failed to send data to client: {}", err);
                        }
                    }
                }
            }
        }
    }
}

macro_rules! read_more {
    ($client:ident, $err:literal) => {
        match $client.read() {
            Ok(OK)
                => {},
            Ok(AGAIN)
                => return Ok(AGAIN),
            Ok(DECLINED)
                => return http_fatal!($err),
            Err(err)
                => return http_fatal!(err.what())
        }
    }
}