/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::ops::{ Deref, DerefMut };
use mio::net::TcpStream;
use std::net::{ SocketAddr, Shutdown };
use std::os::unix::io::{ IntoRawFd, FromRawFd, AsRawFd };
use std::time::{ SystemTime, Duration };
use mio::event::Source;
use mio::{ Interest, Registry, Token };
use std::io;

use crate::error::CoreError;

pub struct TcpSocket {
    stream: Option<TcpStream>,
    owned: bool,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    pub (crate) exp: Option<SystemTime>
}

impl Deref for TcpSocket {
    type Target = TcpStream;
    fn deref(&self) -> &Self::Target {
        match &self.stream {
            Some(stream) => stream,
            None => unreachable!()
        }
    }
}

impl DerefMut for TcpSocket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.stream {
            Some(stream) => stream,
            None => unreachable!()
        }
    }
}

impl Source for TcpSocket {
    fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        match &mut self.stream {
            Some(stream) => stream.register(registry, token, interests),
            None => unreachable!()
        }
    }

    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        match &mut self.stream {
            Some(stream) => stream.reregister(registry, token, interests),
            None => unreachable!()
        }
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        match &mut self.stream {
            Some(stream) => stream.deregister(registry),
            None => unreachable!()
        }
    }
}

impl TcpSocket {
    pub fn from(stream: TcpStream) -> Result<TcpSocket, CoreError> {
        Ok(TcpSocket {
            local_addr: stream.local_addr().or_else(|err| throw!(err))?,
            remote_addr: stream.peer_addr().or_else(|err| throw!(err))?,
            stream: Some(TcpStream::from(stream)),
            owned: true,
            exp: None
        })
    }

    pub fn connect(addr: SocketAddr, timeout: Option<Duration>) -> Result<TcpSocket, CoreError> {
        let stream = TcpStream::connect(addr).or_else(|err| throw!("Failed to proxy connect: {}", err))?;
        Ok(TcpSocket {
            local_addr: stream.local_addr().or_else(|err| throw!(err))?,
            remote_addr: stream.peer_addr().or_else(|err| throw!(err))?,
            stream: Some(TcpStream::from(stream)),
            owned: true,
            exp: match timeout {
                Some(timeout) => Some(SystemTime::now() + timeout),
                None => None
            }
        })
    }

    pub fn weak(&self) -> TcpSocket {
        TcpSocket {
            stream: Some(unsafe { TcpStream::from_raw_fd(self.as_raw_fd()) }),
            owned: false,
            local_addr: self.local_addr,
            remote_addr: self.remote_addr,
            exp: self.exp
        }
    }

    pub fn is_weak(&self) -> bool {
        !self.owned
    }

    pub fn take(&mut self) -> TcpSocket {
        let owned = self.owned;
        self.owned = false;
        TcpSocket {
            stream: self.stream.take(),
            owned: owned,
            local_addr: self.local_addr,
            remote_addr: self.remote_addr,
            exp: self.exp
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn close(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    pub fn valid(&self) -> bool {
        if let Some(stream) = &self.stream {
            if let Ok(None) = stream.take_error() {
                return true
            }
        }
        false
    }

    pub fn timedout(&self) -> bool {
        match self.exp {
            Some(exp) => exp <= SystemTime::now(),
            None => false
        }
    }

    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> Option<SystemTime> {
        self.exp = match timeout {
            Some(timeout) => Some(SystemTime::now() + timeout),
            None => None
        };
        self.exp
    }

    pub fn exp(&self) -> Option<SystemTime> {
        self.exp
    }

    pub fn expire(&mut self, exp: Option<SystemTime>) {
        self.exp = exp
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        if !self.owned {
            self.stream.take().map(|stream| stream.into_raw_fd());
        }
    }
}