/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::any::Any;
use std::cmp::Ordering;
use std::ops::{ Deref, DerefMut };
use std::collections::{ BTreeSet, HashMap };
use std::sync::{ mpsc, Once, Arc, Mutex, atomic::AtomicUsize, atomic };
use mio::net::TcpStream;
use std::net::SocketAddr;
use std::io::ErrorKind;
use mio::{ Events, Interest, Poll, Token, Waker };
use std::time::{ SystemTime, Duration };

use crate::error::CoreError;
use crate::tcp_socket::TcpSocket;

const KEEPALIVE_TIMEOUT_DEFAULT: u64 = 86400;

pub type StreamType = TcpSocket;

pub struct Peer {
    upstream: Option<String>,
    pool: Option<ConnectionPool>,
    active: Option<Arc<i8>>,
    keepalive: Option<Arc<i8>>,
    token: Token,
    userdata: Option<Box<dyn Any + Send>>,
    requests: u64,
    pub stream: StreamType
}

enum Message {
    Add(Peer),
    Remove(Peer)
}

pub struct ConnectionPool {
    max_keepalive: usize,
    max_active: usize,
    name: String,
    active: Arc<i8>,
    keepalive: Arc<i8>,
    timeout: Option<Duration>,
    keepalive_timeout: Duration,
    keepalive_requests: u64,
    peers: Arc<Mutex<BTreeSet<Peer>>>,
    monitor: Arc<Mutex<mpsc::Sender<Message>>>
}

impl Eq for Peer {}

impl PartialOrd for Peer {
    fn partial_cmp(&self, other: &Peer) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Peer {
    fn cmp(&self, other: &Peer) -> Ordering {
        match self.stream.exp.cmp(&other.stream.exp) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => self.token.cmp(&other.token)
        }
    }
}

impl PartialEq for Peer {
    fn eq(&self, other: &Peer) -> bool {
        self.token.eq(&other.token) && self.stream.exp.eq(&other.stream.exp)
    }
}

impl Clone for ConnectionPool {
    fn clone(&self) -> ConnectionPool {
        ConnectionPool {
            max_keepalive: self.max_keepalive,
            max_active: self.max_active,
            name: self.name.clone(),
            active: Arc::new(-1),
            keepalive: Arc::new(-1),
            timeout: self.timeout,
            keepalive_timeout: self.keepalive_timeout,
            keepalive_requests: self.keepalive_requests,
            peers: Arc::clone(&self.peers),
            monitor: self.monitor.clone()
        }
    }
}

static INIT: Once = Once::new();
static mut TX: Option<mpsc::Sender<Message>> = None;
static mut SIGNALLER: Option<Waker> = None;

impl ConnectionPool {
    pub fn new(name: &str, max_keepalive: usize,  max_active: usize) -> ConnectionPool {
        ConnectionPool::with_timeouts(name, max_keepalive, max_active, None, None, None)
    }

    fn send(&self, message: Message) {
        if self.monitor.lock().unwrap().send(message).is_ok() {
            unsafe {
                let _ = SIGNALLER.as_ref().unwrap().wake();
            }
        }
    }

    pub fn with_timeouts(
        name: &str,
        max_keepalive: usize,
        max_active: usize,
        timeout: Option<Duration>,
        keepalive_timeout: Option<Duration>,
        keepalive_requests: Option<u64>
    ) -> ConnectionPool {
        INIT.call_once(|| {
            let rx = unsafe {
                let (tx, rx) = mpsc::channel();
                TX = Some(tx);
                rx
            };

            const SIGNAL: Token = Token(0);

            let mut poll = Poll::new().unwrap();
            let mut events = Events::with_capacity(10240);

            unsafe {
                SIGNALLER = Some(Waker::new(poll.registry(), SIGNAL).expect("Failed to register signaller"));
            };

            fn retain_timedout(poll: &Poll, keepalive: &mut BTreeSet<Peer>) {
                loop {
                    let peer = match keepalive.iter().next() {
                        Some(peer) if !peer.timedout()
                            => return,
                        Some(peer)
                            => peer.weak(),
                        None
                            => return
                    };

                    let mut peer = keepalive.take(&peer).unwrap();

                    log_error!("info", "Keep-alived connection remote={} local={} timedout",
                               peer.remote_addr(), peer.local_addr());

                    let _ = poll.registry().deregister(&mut peer.stream);
                    ConnectionPool::remove_keepalive(&mut peer);
                }
            }

            std::thread::Builder::new().name("ws: keepalive".to_string()).spawn(move || {
                let mut keepalive: BTreeSet<Peer> = BTreeSet::new();

                loop {
                    let timeout = match keepalive.iter().next() {
                        Some(peer) => match peer.exp() {
                            Some(exp)
                                // may be already expired
                                => exp.duration_since(SystemTime::now()).unwrap_or(Duration::from_secs(0)),
                            None
                                => Duration::from_secs(1)
                        },
                        None => Duration::from_secs(1)
                    };

                    match poll.poll(&mut events, Some(timeout)) {

                        Ok(()) if events.is_empty() => {
                            /* no events */
                            retain_timedout(&poll, &mut keepalive);
                        },

                        Ok(()) => {
                            let mut tokens = HashMap::new();

                            events.iter().for_each(|event| {
                                match event.token() {
                                    SIGNAL => return,
                                    token => tokens.insert(token, event)
                                };
                            });

                            if !tokens.is_empty() {
                                keepalive = keepalive.into_iter().filter_map(|mut peer| {
                                    match tokens.remove(&peer.token()) {
                                        Some(event) if event.is_read_closed() => {
                                            log_error!("info", "Keep-alived connection remote={} local={} has closed",
                                                       peer.remote_addr(), peer.local_addr());
                                        },
                                        Some(event) if event.is_error() => {
                                            log_error!("error", "Keep-alived connection remote={} local={} has closed by error",
                                                       peer.remote_addr(), peer.local_addr());
                                        },
                                        _ => {
                                            if peer.timedout() {
                                                let _ = poll.registry().deregister(&mut peer.stream.weak());
                                                log_error!("info", "Keep-alived connection remote={} local={} timedout",
                                                           peer.remote_addr(), peer.local_addr());
                                                ConnectionPool::remove_keepalive(&mut peer);
                                                return None
                                            }
                                            return Some(peer)    
                                        }
                                    }

                                    let _ = poll.registry().deregister(&mut peer.stream.weak());

                                    ConnectionPool::remove_keepalive(&mut peer);

                                    None
                                }).collect();
                            }
                        },

                        Err(err) => match err.kind() {
                            ErrorKind::TimedOut | ErrorKind::Interrupted => retain_timedout(&poll, &mut keepalive),
                            err => log_error!("error", "Poll has failed: {:?}", err)
                        }
                    }

                    loop {
                        match rx.try_recv() {
                            Ok(Message::Remove(peer)) => {
                                if let Some(mut peer) = keepalive.take(&peer) {
                                    let _ = poll.registry().deregister(&mut peer.stream);
                                    peer.release();
                                    continue;
                                }
                            },
                            Ok(Message::Add(mut peer)) => {
                                // add connection to monitor
                                if peer.stream.valid() && keepalive.len() < 10240 {
                                    let token = peer.token();
                                    match poll.registry().register(&mut peer.stream, token, Interest::READABLE) {
                                        Ok(()) => {
                                            keepalive.insert(peer);
                                        },
                                        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                                            peer.release();
                                        },
                                        Err(err) => {
                                            log_error!("error", "Failed to register keep_alive event: {}", err)
                                        }
                                    }
                                }
                            },
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(err) => {
                                log_error!("error", "Failed to recv from channel: {:?}", err);
                                break;
                            }
                        }
                    }
                }
            }).unwrap();
        });

        let tx = unsafe {
            match &TX {
                Some(tx) => tx.clone(),
                None => unreachable!()
            }
        };

        ConnectionPool {
            max_keepalive: if max_keepalive == 0 { std::usize::MAX } else { max_keepalive },
            max_active: if max_active == 0 { std::usize::MAX } else { max_active },
            name: name.to_string(),
            active: Arc::new(0),
            keepalive: Arc::new(0),
            timeout: timeout,
            keepalive_timeout: keepalive_timeout.unwrap_or(Duration::from_secs(KEEPALIVE_TIMEOUT_DEFAULT)),
            keepalive_requests: keepalive_requests.unwrap_or(std::u64::MAX),
            peers: Arc::new(Mutex::new(BTreeSet::new())),
            monitor: Arc::new(Mutex::new(tx))
        }
    }

    pub fn update_max_active(&mut self, max_active: usize) {
        self.max_active = max_active
    }

    pub fn update_max_keepalive(&mut self, max_keepalive: usize) {
        self.max_keepalive = max_keepalive
    }

    pub fn active(&self) -> usize {
        Arc::strong_count(&self.active) - 1
    }

    pub fn idle(&self) -> usize {
        Arc::strong_count(&self.keepalive) - Arc::strong_count(&self.active)
    }

    pub fn connect(&self, addr: &SocketAddr, timeout: Option<Duration>) -> Result<Peer, CoreError> {
        let mut guard = self.peers.lock().unwrap();
        let peers = &mut * guard;

        if self.active() == self.max_active {
            return throw!("max_active has been reached to {}", self.name);
        }

        loop {
            let peer = match peers.iter().next() {
                Some(peer) => peer.weak(),
                None => {
                    let stream = StreamType::connect(*addr, timeout.or(self.timeout)).or_else(|err| throw!(err))?;
                    let mut peer = Peer::new(stream, Some(self.name.clone()));
                    peer.pool = Some(self.clone());
                    peer.active = Some(Arc::clone(&self.active));
                    peer.keepalive = Some(Arc::clone(&self.keepalive));
                    return Ok(peer);
                }
            };

            let mut peer = peers.take(&peer).unwrap();
            if !peer.stream.valid() {
                continue;
            }

            drop(peers);

            self.send(Message::Remove(peer.weak()));

            peer.set_timeout(timeout.or(self.timeout));
            peer.pool = Some(self.clone());
            peer.active = Some(Arc::clone(&self.active));
            peer.keepalive = Some(Arc::clone(&self.keepalive));
            peer.token = next_token();

            return Ok(peer);
        }
    }

    fn set_keepalive(&self, mut peer: Peer, timeout: Option<Duration>) {
        if !peer.stream.valid() {
            return;
        }

        let peer = self.peers.lock().and_then(|mut peers| {
            if self.idle() == self.max_keepalive {
                return Ok(None);
            }

            peer.requests += 1;

            if peer.requests == self.keepalive_requests {
                log_error!("info", "Keep-alived connection remote={} local={} has closed (keepalive_requests)",
                           peer.remote_addr(), peer.local_addr());
                return Ok(None);
            }

            peer.active = None;

            let mut reuse = peer.weak();

            peer.expire(reuse.set_timeout(Some(timeout.unwrap_or(self.keepalive_timeout))));

            peers.insert(peer);

            Ok(Some(reuse))
        }).unwrap();

        if let Some(mut peer) = peer {
            peer.pool = Some(self.clone());
            self.send(Message::Add(peer));
        }
    }

    fn remove_keepalive(peer: &mut Peer) {
        if let Some(pool) = peer.pool.take() {
            pool.peers.lock().unwrap().remove(&peer);
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.userdata = None;
        if let Some(pool) = self.pool.take() {
            pool.set_keepalive(self.take(), None);
        }
    }
}

impl Deref for Peer {
    type Target = TcpStream;
    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for Peer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}

impl Peer {
    pub fn new(stream: StreamType, upstream: Option<String>) -> Peer {
        Peer {
            upstream: upstream,
            pool: None,
            active: None,
            keepalive: None,
            token: next_token(),
            stream: stream,
            userdata: None,
            requests: 0
        }
    }

    pub fn upstream(&self) -> String {
        match self.upstream.as_ref() {
            Some(upstream) => upstream.clone(),
            None => self.remote_addr().to_string()
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.stream.local_addr()
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.stream.remote_addr()
    }

    pub fn attach_userdata(&mut self, userdata: Box<dyn Any + Send + Sync>) {
        self.userdata = Some(userdata);
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn set_keepalive(mut self, timeout: Option<Duration>) {
        self.userdata = None;
        if let Some(pool) = self.pool.take() {
            pool.set_keepalive(self, timeout);
        }
    }

    pub fn close(&mut self) {
        let _ = self.stream.close();
        self.pool = None;
    }

    pub fn take(&mut self) -> Peer {
        Peer {
            upstream: self.upstream.take(),
            pool: self.pool.take(),
            active: self.active.take(),
            keepalive: self.keepalive.take(),
            token: self.token,
            stream: self.stream.take(),
            userdata: self.userdata.take(),
            requests: self.requests
        }
    }

    pub fn weak(&self) -> Peer {
        Peer {
            upstream: self.upstream.clone(),
            pool: None,
            active: None,
            keepalive: None,
            token: self.token,
            stream: self.stream.weak(),
            userdata: None,
            requests: self.requests
        }
    }

    pub fn release(&mut self) {
        self.pool = None;
    }

    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> Option<SystemTime> {
        self.stream.set_timeout(timeout)
    }

    pub fn expire(&mut self, exp: Option<SystemTime>) {
        self.stream.expire(exp)
    }

    pub fn exp(&self) -> Option<SystemTime> {
        self.stream.exp()
    }

    pub fn timedout(&self) -> bool {
        self.stream.timedout()
    }
}

fn next_token() -> Token {
    static mut UNIQUE_TOKEN: AtomicUsize = AtomicUsize::new(1);
    unsafe {
        Token(UNIQUE_TOKEN.fetch_add(1, atomic::Ordering::SeqCst))
    }
}