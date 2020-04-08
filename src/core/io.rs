/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use net2::unix::UnixTcpBuilderExt;
use std::collections::{ LinkedList, HashMap, BTreeSet };
use std::io::{ Error, ErrorKind };
use std::sync::{ Arc, Mutex };
use std::sync::atomic::{ AtomicBool, Ordering };
use std::{ thread, thread::JoinHandle };
use std::time::{ Duration, SystemTime };
use std::net::SocketAddr;
use mio::net::TcpListener;
use mio::{ Events, Interest, Poll, Token, Registry, Waker };
use uuid::Uuid;

use crate::client_context::*;
use crate::module::*;
use crate::core::{ *, worker::ThreadPool };
use crate::error::{ *, Code::* };
use crate::connection_pool::{ Peer, StreamType };

const SIGNAL: Token = Token(0);
const SERVER: Token = Token(1);
const CLIENT: Token = Token(100000);

enum OneOf {
    Invalid(SocketAddr),
    Valid(TcpListener)
}

enum Server {
    Invalid((SocketAddr, Options, Token)),
    Valid((TcpListener, Options, Token)),
    Removed(OneOf)
}

enum Item<T: ModuleType + 'static> {
    Idle(ClientContext),
    Request(T::Request),
    Response((T::Response, Option<Peer>))
}

pub (crate) struct IO {
    thr: Option<JoinHandle<()>>,
    server_token: Token,
    servers: Arc<Mutex<HashMap<Token, Server>>>,
    stop: Arc<AtomicBool>,
    updated: Arc<AtomicBool>
}

impl IO {

    pub fn new<T: ModuleType + 'static, F: 'static>(
        worker_pool_size: usize,
        socket_poll_size: usize,
        handler: F
    )
        -> Result<IO, CoreError>
    where
        F: Fn(T::Request) -> T::Response + Clone + Sync + Send
    {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(socket_poll_size);

        let (ready, ready_) = pair(|| Mutex::new(LinkedList::new()));
        let (servers, servers_) = pair(|| Mutex::new(HashMap::new()));

        let signaller = Arc::new(Waker::new(poll.registry(), SIGNAL).expect("Failed to register signaller"));
        let signaller_ = Arc::clone(&signaller);

        let mut clients: HashMap<Token, Item<T>> = HashMap::new();
        let mut keepalive: BTreeSet<(SystemTime, Token)> = BTreeSet::new();

        let mut unique_token = CLIENT;
        let server_token = next(&mut SERVER);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_ = stop.clone();

        let updated = Arc::new(AtomicBool::new(true));
        let updated_ = updated.clone();

        let mut workers = ThreadPool::<T, _>::new(worker_pool_size, move |r| {
            ready_.lock().unwrap().push_back(handler(r));
            signaller_.wake().expect("Failed to wake up poll");
        });

        let thr = thread::Builder::new().name("ws: io".to_string()).spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if updated.load(Ordering::Acquire) {
                    if let Ok(ref mut servers) = servers.lock() {
                        IO::update_servers(&mut poll, servers);
                        updated.store(false, Ordering::Release);
                    }
                }

                // keepalived

                let now = SystemTime::now();
                let mut timeout = Duration::from_secs(1);

                loop {
                    let key = match keepalive.iter().next() {
                        Some((exp, _)) if *exp > now => {
                            timeout = exp.duration_since(SystemTime::now()).unwrap_or(Duration::from_secs(0));
                            break;
                        },
                        Some(key) => key.clone(),
                        None => break
                    };

                    if let Some(client) = clients.remove(&keepalive.take(&key).unwrap().1) {
                        match client {
                            Item::Idle(mut client) => {
                                log_error!("info", "Client keep-alived connection client={} local={} timedout",
                                           &client.remote_addr(), &client.local_addr());
                                deregister(poll.registry(), &mut client);
                            },
                            Item::Request(mut r) => {
                                log_error!("warn", "Client connection client={} local={} request timedout",
                                           r.context().remote_addr(), r.context().local_addr());
                                deregister(poll.registry(), r.context());
                                r.on_timedout();
                            },
                            Item::Response((mut resp, None)) => {
                                log_error!("warn", "Client connection client={} local={} response timedout",
                                           resp.context().remote_addr(), resp.context().local_addr());
                                deregister(poll.registry(), &mut resp.context());
                                resp.on_timedout();
                            },
                            Item::Response((mut resp, Some(mut peer))) => {
                                log_error!("warn", "Client connection client={} local={} peer={} response timedout",
                                           resp.context().remote_addr(), resp.context().local_addr(), peer.remote_addr());
                                deregister(poll.registry(), &mut peer.stream);
                                deregister(poll.registry(), &mut resp.context());
                                resp.on_timedout();
                            }
                        }
                    }
                }

                if let Err(err) = poll.poll(&mut events, Some(timeout)) {
                    match err.kind() {
                        ErrorKind::TimedOut | ErrorKind::Interrupted => { /* skip */ },
                        other => log_error!("error", "Poll has failed: {:?}", other)
                    }
                    continue;
                }

                for event in events.iter() {
                    match event.token() {
                        SIGNAL => {

                            // Content phase completed

                            let mut ready = ready.lock().unwrap();

                            while let Some(mut resp) = ready.pop_front() {
                                let token = next(&mut unique_token);
                                if register(poll.registry(), resp.context(), token, Interest::WRITABLE) {
                                    let response_timeout = resp.context().inner.as_ref().unwrap().opts.response_timeout;
                                    if let Some(exp) = resp.set_timeout(response_timeout) {
                                        keepalive.insert((exp, token));
                                    }
                                    clients.insert(token, Item::Response((resp, None)));
                                }
                            }
                        },

                        token if token.0 < CLIENT.0 => {

                            // New client

                            let mut servers = servers.lock().unwrap();

                            if let Some(server) = servers.remove(&token) {
                                if let Server::Valid((mut listener, opts, server_token)) = server  {
                                    let client_token = next(&mut unique_token);
                                    match IO::handle_accept(&mut poll, &mut listener, client_token, &opts) {
                                        Ok(mut client) => {
                                            if let Err(err) = poll.registry().reregister(&mut listener, server_token, Interest::READABLE) {
                                                log_error!("error", err);
                                            }
                                            if let Some(exp) = client.set_timeout(opts.request_timeout) {
                                                keepalive.insert((exp, token));
                                            }
                                            clients.insert(client_token, Item::Idle(client));
                                            servers.insert(server_token, Server::Valid((listener, opts, server_token)));
                                        },
                                        Err(DECLINED) => {
                                            /* may be no space in poll ? */
                                            servers.insert(server_token, Server::Valid((listener, opts, server_token)));
                                        },
                                        Err(AGAIN) => {
                                            let server_addr = listener.local_addr();
                                            match IO::create_listener(OneOf::Valid(listener), server_token, &mut poll) {
                                                Ok(listener) => {
                                                    servers.insert(server_token, Server::Valid((listener, opts, server_token)));
                                                },
                                                Err(err) => {
                                                    servers.insert(server_token, Server::Invalid((server_addr.unwrap(), opts, server_token)));
                                                    log_error!("error", "Failed to create listener: {}", err);
                                                }
                                            }
                                        },
                                        Err(OK) => panic!("Unreachable")
                                    }
                                }
                            }
                        },

                        token => {
                            IO::handle_io::<T, _>(
                                &poll,
                                token,
                                &mut clients,
                                &mut keepalive,
                                &workers
                            );
                        }
                    }
                }
            }

            workers.stop();
            workers.wait();
        }).unwrap();

        return Ok(IO {
            thr: Some(thr),
            servers: servers_,
            server_token: server_token,
            stop: stop_,
            updated: updated_
        });
    }

    pub fn add_listener(&mut self, addr: SocketAddr, opts: Option<Options>) -> CoreResult {
        let mut token = None;
        let mut servers = self.servers.lock().unwrap();

        for (server_token, server) in servers.iter() {
            match server {
                Server::Valid((listener,..)) => {
                    if listener.local_addr().unwrap() == addr {
                        token = Some(*server_token);
                        break;
                    }
                },
                Server::Invalid((server_addr,..)) => {
                    if *server_addr == addr {
                        token = Some(*server_token);
                        break;
                    }
                },
                Server::Removed(server) => {
                    match server {
                        OneOf::Invalid(server_addr) => {
                            if *server_addr == addr {
                                token = Some(*server_token);
                                break;
                            }
                        },
                        OneOf::Valid(listener) => {
                            if listener.local_addr().unwrap() == addr {
                                token = Some(*server_token);
                                break;
                            }
                        }
                    }
                }
            }
        }

        let res = match token {
            Some(token) => {
                // found
                match servers.remove(&token).unwrap() {
                    Server::Valid(server) => {
                        servers.insert(token, Server::Valid(server));
                        Ok(DECLINED)
                    },
                    Server::Invalid(server) => {
                        servers.insert(token, Server::Invalid(server));
                        Ok(DECLINED)
                    },
                    Server::Removed(server) => {
                        match server {
                            OneOf::Valid(listener) => {
                                servers.insert(token, Server::Valid((listener, opts.unwrap_or_default(), token)));
                            },
                            OneOf::Invalid(server_addr) => {
                                servers.insert(token, Server::Invalid((server_addr, opts.unwrap_or_default(), token)));
                            }
                        }
                        Ok(OK)
                    }
                }
            },
            None => {
                // new one
                let token = next(&mut self.server_token);
                servers.insert(token, Server::Invalid((addr, opts.unwrap_or_default(), token)));
                Ok(OK)
            }
        };

        self.updated.store(true, Ordering::Release);

        res
    }

    pub fn remove_listener(&mut self, addr: SocketAddr) {
        let mut servers = self.servers.lock().unwrap();

        if let Some(token) = servers.values().filter_map(|server| {
            match server {
                Server::Valid((listener, _, token)) => {
                    return if listener.local_addr().unwrap() == addr { 
                        Some(*token)
                    } else {
                        None
                    }
                },
                Server::Invalid((server_addr, _, token)) => {
                    return if *server_addr == addr { 
                        Some(*token)
                    } else {
                        None
                    }
                },
                Server::Removed(_) => None
            }
        }).nth(0) {
            if let Some(server) = servers.remove(&token) {
                match server {
                    Server::Valid((listener,..)) => {
                        servers.insert(token, Server::Removed(OneOf::Valid(listener)));
                    },
                    Server::Invalid((server_addr,..)) => {
                        servers.insert(token, Server::Removed(OneOf::Invalid(server_addr)));
                    },
                    Server::Removed(_) => panic!("Unreachable")
                }
            }
        }

        self.updated.store(true, Ordering::Release);
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn wait(&mut self) {
        self.thr.take().unwrap().join().unwrap();
    }

    fn update_servers(
        poll: &mut Poll,
        servers: &mut HashMap<Token, Server>
    ) {
        servers.retain(|_, server| {
            if let Server::Removed(state) = server {
                if let OneOf::Valid(ref mut listener) = state {
                    poll.registry().deregister(listener).unwrap();
                }
                false
            } else {
                true
            }
        });

        for (token, server) in servers.iter_mut() {
            if let Server::Invalid((addr, opts, _)) = server {
                match IO::create_listener(OneOf::Invalid(*addr), *token, poll) {
                    Ok(listener) => *server = Server::Valid((listener, opts.clone(), *token)),
                    Err(err) => log_error!("error", "Failed to create listener: {}", err)
                }
            }
        }
    }

    fn create_listener(
        listen: OneOf,
        token: Token,
        poll: &mut Poll
    ) -> Result<TcpListener, Error> {
        let addr = match listen {
            OneOf::Valid(mut listener) => {
                poll.registry().deregister(&mut listener).unwrap();
                let addr = listener.local_addr().unwrap();
                drop(listener);
                addr
            },
            OneOf::Invalid(addr) => addr,
        };

        let mut listener = TcpListener::from_std(net2::TcpBuilder::new_v4()?.reuse_address(true)?.reuse_port(true)?.bind(addr)?.listen(512)?);

        poll.registry().register(&mut listener, token, Interest::READABLE)?;

        Ok(listener)
    }

    fn handle_accept(
        poll: &mut Poll,
        server: &mut TcpListener,
        token: Token,
        opts: &Options
    ) -> Result<ClientContext, Code> {
        match server.accept() {
            Ok((mut stream, _)) => {
                match poll.registry().register(&mut stream, token, Interest::READABLE) {
                    Ok(()) => {
                        Ok(ClientContext::with_state(StreamType::from(stream).or_else(|err| {
                               log_error!("error", "Failed to create client context: {}", err);
                               Err(DECLINED)
                           })?,
                           server.local_addr().unwrap(),
                           State {
                               requests: 0,
                               opts: opts.clone(),
                               request_id: Uuid::new_v4()
                           }))
                    },
                    Err(err) =>  {
                        log_error!("error", "Failed to register read event for client socket: {}", err);
                        Err(DECLINED)
                    }
                }
            },
            Err(err) => {
                log_error!("error", "Failed to accept: {}", err);
                Err(AGAIN)
            }
        }
    }

    fn handle_io<T: ModuleType, F: 'static>(
        poll: &Poll,
        token: Token,
        clients: &mut HashMap<Token, Item<T>>,
        keepalive: &mut BTreeSet<(SystemTime, Token)>,
        workers: &ThreadPool<T, F>
    )
    where
        T::Request: Send,
        T::Response: Send,
        F: Fn(T::Request) + Clone + Sync + Send
    {
        loop {
            match clients.remove(&token) {

                None => break,

                Some(Item::Idle(mut client)) => {
                    if let Some(exp) = client.exp() {
                        keepalive.remove(&(exp, token));
                    }
                    let request_timeout = client.inner.as_ref().unwrap().opts.request_timeout;
                    if let Some(exp) = client.set_timeout(request_timeout) {
                        keepalive.insert((exp, token));
                    }
                    let mut inner = client.inner.as_mut().unwrap();
                    inner.request_id = Uuid::new_v4();
                    clients.insert(token, Item::Request(T::Request::new(client)));
                },

                Some(Item::Request(mut r)) => {
                    if let Some(exp) = r.context().exp() {
                        keepalive.remove(&(exp, token));
                    }
                    return match r.parse() {
                        Ok(OK) => {
                            // request has received
                            deregister(poll.registry(), r.context());
                            r.context().reset();
                            if let Err(err) = workers.post(r) {
                                log_error!("error", err);
                            }
                        },
                        Ok(AGAIN) => {
                            // continue receiving request
                            if let Some(exp) = r.context().exp() {
                                keepalive.insert((exp, token));
                            }
                            clients.insert(token, Item::Request(r));
                        },
                        Ok(DECLINED) => {
                            // closed
                            log_error!("info", "Keep-alived connection client={} local={} has closed",
                                       r.context().remote_addr(), r.context().local_addr());
                            deregister(poll.registry(), r.context());
                        }
                        Err(err) => {
                            deregister(poll.registry(), r.context());
                            log_error!("error", "{} client={} local={}", err, r.context().remote_addr(), r.context().local_addr());
                        }
                    }
                },

                Some(Item::Response((mut resp, _))) => {
                    if let Some(exp) = resp.context().exp() {
                        keepalive.remove(&(exp, token));
                    }
                    loop {
                        match resp.flush() {
                            Ok(Flush::OK(None)) => {
                                // request completed
                                if register(poll.registry(), resp.context(), token, Interest::READABLE) {
                                    let mut client = resp.close();
                                    let keepalive_timeout = match &mut client.inner {
                                        Some(state) => {
                                            state.requests += 1;
                                            if state.requests == state.opts.keepalive_requests {
                                                // close keep-alive session
                                                log_error!("info", "Client keep-alived connection client={} local={} has closed (keepalive_requests)",
                                                           client.remote_addr(), client.local_addr());
                                                return;
                                            }
                                            state.opts.keepalive_timeout
                                        },
                                        None => None
                                    };
                                    client.reset();
                                    if let Some(exp) = client.set_timeout(keepalive_timeout) {
                                        keepalive.insert((exp, token));
                                    }
                                    clients.insert(token, Item::Idle(client));
                                }
                            },
                            Ok(Flush::OK(Some(mut peer))) => {
                                // request completed (additional keep-alive socket must be deregistered)
                                deregister(poll.registry(), &mut peer.stream);
                                continue;
                            },
                            Ok(Flush::READ_MORE(mut peer)) => {
                                // need more data
                                if register(poll.registry(), &mut peer.stream, token, Interest::READABLE) {
                                    if let Some(exp) = resp.context().exp() {
                                        keepalive.insert((exp, token));
                                    }
                                    clients.insert(token, Item::Response((resp, Some(peer))));
                                }
                            },
                            Ok(Flush::WRITE_MORE(mut peer)) => {
                                // need more data
                                if register(poll.registry(), &mut peer.stream, token, Interest::WRITABLE) {
                                    if let Some(exp) = resp.context().exp() {
                                        keepalive.insert((exp, token));
                                    }
                                    clients.insert(token, Item::Response((resp, Some(peer))));
                                }
                            },
                            Ok(Flush::READ_WRITE_MORE(mut peer)) => {
                                // need more data
                                if register(poll.registry(), &mut peer.stream, token, Interest::READABLE | Interest::WRITABLE) {
                                    if let Some(exp) = resp.context().exp() {
                                        keepalive.insert((exp, token));
                                    }
                                    clients.insert(token, Item::Response((resp, Some(peer))));
                                }
                            },
                            Ok(Flush::AGAIN) => {
                                // need more data
                                if register(poll.registry(), resp.context(), token, Interest::WRITABLE) {
                                    if let Some(exp) = resp.context().exp() {
                                        keepalive.insert((exp, token));
                                    }
                                    clients.insert(token, Item::Response((resp, None)));
                                }
                            },
                            Ok(Flush::DECLINED) => {
                                // closed
                                deregister(poll.registry(), resp.context());
                            }
                            Err(err) => {
                                log_error!("error", "Failed to send response: {}", err);
                            }
                        }
                        return;
                    }        
                }
            }
        }
    }
}

fn pair<T, F: 'static>(f: F) -> (Arc<T>, Arc<T>)
where
    F: Fn() -> T
{
    let object = Arc::new(f());
    let copied = Arc::clone(&object);
    (object, copied)
}

fn next(current: &mut Token) -> Token {
    let next = current.0;
    current.0 += 1;
    Token(next)
}

fn register(registry: &Registry, stream: &mut StreamType, token: Token, interests: Interest)
    -> bool
{
    match registry.register(stream, token, interests) {
        Ok(()) => true,
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            match registry.reregister(stream, token, interests) {
                Ok(()) => true,
                Err(err) => {
                    log_error!("error", "Failed to register event: {}", err);
                    false
                }
            }
        }
        Err(err) => {
            log_error!("error", "Failed to register event: {}", err);
            false
        }
    }
}

fn deregister(registry: &Registry, stream: &mut StreamType) {
    let _ = registry.deregister(stream);
}