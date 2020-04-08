/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Proxy);

use std::sync::Arc;
use std::net::SocketAddr;
use std::time::{ Duration, Instant };

use crate::error::*;
use crate::plugin::*;
use crate::config::*;
use crate::http::*;
use crate::http::error::HttpResult;
use crate::connection_pool::*;
use crate::upstream::*;
use crate::http::plugins::upstream::Upstream as HttpUpstream;
use crate::upstream::RoundRobin;
use crate::keyval::Key;
use crate::variable::LazyHandler;

const CRLF: &[u8] = &[ 0x0d, 0x0a ];

const CR: u8 = 0x0D;
const LF: u8 = 0x0A;

#[derive(PartialEq, PartialOrd)]
#[allow(non_camel_case_types)]
enum HttpProxyState {
    st_connecting,
    st_connected,
    st_request_prepared,
    st_request_sent,
    st_protocol,
    st_protocol_end,
    st_status,
    st_status_end,
    st_headers,
    st_headers_end,
    st_body,
    st_parsed
}

struct HttpProxyContext {
    timer: Instant,
    client: ClientContext,
    peer: Peer,
    state: HttpProxyState,
    status: Vec<u8>,
    protocol: Vec<u8>,
    key: Option<Vec<u8>>,
    val: Option<Vec<u8>>,
    chunk: (Vec<u8>, Option<usize>)
}

impl HttpProxyContext {
    fn new(peer: Peer) -> HttpProxyContext {
        HttpProxyContext {
            timer: Instant::now(),
            client: ClientContext::new(peer.stream.weak(), peer.remote_addr()),
            peer: peer,
            state: HttpProxyState::st_connecting,
            status: Vec::with_capacity(64),
            protocol: Vec::with_capacity(16),
            key: Some(Vec::with_capacity(64)),
            val: None,
            chunk: (Vec::with_capacity(256), None)
        }
    }

    fn prepare_request(&mut self, r: &mut HttpRequest) -> CoreResult {
        if self.state > HttpProxyState::st_request_prepared {
            return Ok(OK);
        }

        let client = &mut self.client;

        client.write_str(&format!("{} ", r.method()));
        client.write_str(&r.uri());
        if !r.args_mut().is_empty() {
            client.write(b"?");
            client.write_str(&r.format_args());
        }
        client.write(b" HTTP/1.1\r\n");

        r.headers_mut().remove("connection");

        for (key, ll) in r.headers().iter() {
            for v in ll.iter() {
                client.write_str(&format!("{}: {}\r\n", key, &v));
            }
        }

        client.write(CRLF);

        if let Some(body) = r.body() {
            client.write(body);
        }

        self.state = HttpProxyState::st_request_prepared;

        Ok(OK)
    }

    fn send_request(&mut self, r: &mut HttpRequest) -> CoreResult {
        match self.prepare_request(r) {
            Ok(OK) => {
                if self.state < HttpProxyState::st_request_sent {
                    return match self.client.flush() {
                        Ok((AGAIN, _)) => {
                            Ok(AGAIN)
                        },
                        Ok((OK, _)) => {
                            self.client.reset();
                            self.state = HttpProxyState::st_request_sent;
                            Ok(OK)
                        },
                        Err(err) => throw!(err.what()),
                        Ok((DECLINED, _)) => unreachable!()
                    }
                }
                Ok(OK)
            },
            Ok(AGAIN) => Ok(AGAIN),
            Err(err) => Err(err),
            _ => unreachable!()
        }
    }

    fn parse_response(&mut self, resp: &mut HttpResponse) -> HttpResult {
        match self.parse_protocol()? {
            OK => match self.parse_status(resp)? {
                OK => match self.parse_headers(resp)? {
                    OK => self.read_body(resp),
                    code => Ok(code)
                },
                code => Ok(code)
            },
            code => Ok(code)
        }
    }

    fn parse_status(&mut self, resp: &mut HttpResponse) -> HttpResult {
        let client = &mut self.client;

        if self.state > HttpProxyState::st_status {
            return Ok(OK)
        }

        self.state = HttpProxyState::st_status;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    b' ' if HttpStatus::UNDEFINED == resp.status() => {
                        let status = std::str::from_utf8(&self.status[..])
                                    .or_else(|_| http_throw!("Failed to decode status line"))?
                                    .parse::<i64>()
                                    .or_else(|_| http_throw!("Failed to parse status"))?;
                        resp.set_status(HttpStatus::from(status));
                    }
                    CR => { /* skip */ },
                    LF => {
                        self.state = HttpProxyState::st_status_end;
                        return Ok(OK);
                    },
                    c => self.status.push(c)
                }
            }
            read_more!(client, "Upstream has closed connection on read status line");
        }
    }

    fn parse_protocol(&mut self) -> HttpResult {
        if self.state > HttpProxyState::st_protocol {
            return Ok(OK)
        }

        let client = &mut self.client;

        self.state = HttpProxyState::st_protocol;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    b' ' => {
                        self.state = HttpProxyState::st_protocol_end;
                        match &self.protocol[..] {
                            b"HTTP/1.0" => {
                                self.peer.release();
                                HttpProtocol::HTTP10
                            },
                            b"HTTP/1.1" => HttpProtocol::HTTP11,
                            _ => return http_throw!("Unsupported protocol version")
                        };
                        return Ok(OK);
                    },
                    c => self.protocol.push(c)
                }
            }
            read_more!(client, "Upstream has closed connection on read status line");
        }
    }

    fn parse_headers(&mut self, resp: &mut HttpResponse) -> HttpResult {
        if self.state > HttpProxyState::st_headers {
            return Ok(OK)
        }

        self.state = HttpProxyState::st_headers;

        let client = &mut self.client;

        let mut last = 0u8;
        let mut last_crlf = false;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    LF => {
                        if last != CR {
                            return http_throw!("Invalid header line");
                        }
                        if last_crlf {
                            self.state = HttpProxyState::st_headers_end;
                            return Ok(OK)
                        }
                        last = LF;
                    },
                    CR => {
                        if last == LF {
                            last = CR;
                            last_crlf = true;
                            continue;
                        }

                        if let Some(k) = &self.key {
                            if let Some(v) = &self.val {
                                let name = unsafe { std::str::from_utf8_unchecked(&k) }.trim();
                                let value = unsafe { std::str::from_utf8_unchecked(&v) }.trim();
                                match name.to_ascii_lowercase().as_str() {
                                    "content-length" => {
                                        match value.parse::<usize>() {
                                            Ok(len) => {
                                                resp.set_content_length(len)
                                            },
                                            Err(_) => return http_throw!("Invalid header line")
                                        }
                                    },
                                    "connection" => {
                                      if value.to_ascii_lowercase() == "close" {
                                          self.peer.release();
                                      }
                                    },
                                    "server" => {},
                                    "transfer-encoding" if value.to_ascii_lowercase() == "chunked" => {
                                        resp.set_chunked();
                                    },
                                    _ => {
                                        let ll = resp.headers().entry(Key::from(name)).or_default();
                                        ll.push_back(value.to_string());
                                    }
                                }
                                last = CR;
                                self.key = Some(Vec::with_capacity(64));
                                self.val = None;
                                continue;
                            }
                        }

                        return http_throw!("Invalid header line");
                    },
                    b':' => {
                        if let Some(ref mut v) = &mut self.val {
                            v.push(b':');
                        } else {
                            self.val = Some(Vec::with_capacity(64));
                        }
                    },
                    c => {
                        if let Some(ref mut v) = &mut self.val {
                            v.push(c);
                        } else if let Some(ref mut k) = &mut self.key {
                            assert!(self.val.is_none());
                            k.push(c);
                        }
                        last = c;
                    }
                }
            }
            read_more!(client, "Client has closed connection on read headers");
        }
    }

    fn read_chunk_size(&mut self) -> HttpResult {
        if self.chunk.1.is_some() {
            return Ok(OK)
        }

        let client = &mut self.client;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    CR => { /* skip */ },
                    LF => {
                        self.chunk.1 = match usize::from_str_radix(unsafe {
                            std::str::from_utf8_unchecked(&self.chunk.0)
                        }, 16).or_else(|err| http_throw!("Failed to parse chunk size: {}", err))? {
                            0 => None,
                            size => Some(size)
                        };
                        self.chunk.0.clear();
                        return Ok(OK);
                    },
                    c => self.chunk.0.push(c)
                }
            }
            read_more!(client, "Client has closed connection on read body");
        }
    }

    fn read_chunk(&mut self) -> HttpResult {
        match self.read_chunk_size() {
            Ok(OK) => {
                if let Some(chunk_size) = self.chunk.1 {
                    let client = &mut self.client;
                    loop {
                        while !client.buf.end() {
                            self.chunk.0.push(client.buf.getc());
                            if 2 /* CRLF */ + chunk_size as usize == self.chunk.0.len() {
                                // chunk has readed
                                return Ok(OK);
                            }
                        }
                        read_more!(client, "Client has closed connection on read body");            
                    }
                }
                Ok(OK)
            }
            other => other
        }
    }

    fn read_body(&mut self, resp: &mut HttpResponse) -> HttpResult {
        if self.state > HttpProxyState::st_body {
            return Ok(OK)
        }

        self.state = HttpProxyState::st_body;

        match resp.content_length() {
            Some(content_length) => {
                resp.append_body(self.client.buf.chunk(content_length - resp.body_len()));
                while content_length > resp.body_len() {
                    match self.client.read() {
                        Ok(OK)
                            => resp.append_body(self.client.buf.chunk(content_length - resp.body_len())),
                        Ok(AGAIN)
                            => return Ok(AGAIN),
                        Err(err)
                            => return http_fatal!(err.what()),
                        Ok(DECLINED)
                            => return http_fatal!("Client has closed connection on read body")
                    }
                }
            },
            None if resp.chunked() => {
                loop {
                    match self.read_chunk() {
                        Ok(OK) => {
                            match self.chunk.1 {
                                Some(chunk_size) => {
                                    resp.append_body(&self.chunk.0[..chunk_size]);
                                    self.chunk.0.clear();
                                    self.chunk.1 = None;
                                },
                                None => {
                                    // last chunk
                                    resp.set_content_length(resp.body_len());
                                    break;
                                }
                            }
                        },
                        other => return other
                    }
                }
            },
            None if resp.protocol() == HttpProtocol::HTTP10 => {
                // read to close of stream
                resp.append_body(self.client.buf.tail());
                loop {
                    match self.client.read() {
                        Ok(OK)
                            => resp.append_body(self.client.buf.tail()),
                        Ok(AGAIN)
                            => return Ok(AGAIN),
                        Err(err)
                            => return http_fatal!(err.what()),
                        Ok(DECLINED)
                            => break
                    }
                }
            },
            None if resp.status() == HttpStatus::NOT_MODIFIED => { /* no body */ },
            None => {
                resp.set_status(HttpStatus::BAD_GATEWAY);
                resp.set_content_length(0);
            }
        }

        self.state = HttpProxyState::st_parsed;

        Ok(OK)
    }

    fn proxy(&mut self, resp: &mut HttpResponse) -> FlushResult {
        if self.peer.timedout() && self.state <= HttpProxyState::st_parsed {
            resp.send(HttpStatus::GATEWAY_TIMEOUT, "text/plain", Some(b"Gateway timeout"));
            return Ok(Flush::DECLINED);
        }

        if self.state == HttpProxyState::st_connecting {
            self.state = HttpProxyState::st_connected;
            return Ok(Flush::WRITE_MORE(self.peer.weak()));
        }

        // send request

        match self.send_request(resp.get_request()) {
            Ok(AGAIN)
                => return Ok(Flush::WRITE_MORE(self.peer.weak())),
            Err(err)
                => return Err(err),
            Ok(OK) => {
                // read response
                match self.parse_response(resp) {
                    Ok(OK) => {
                        // send response
                        return Ok(Flush::OK(Some(self.peer.take())));
                    },
                    Ok(AGAIN)
                        => return Ok(Flush::READ_MORE(self.peer.weak())),
                    Err(err)
                        => return throw!(err.what()),
                    Ok(DECLINED)
                        => unreachable!()
                }
            },
            Ok(DECLINED) => unreachable!()
        }
    }
}

#[derive(Default, Clone)]
struct ProxyPass {
    pass: Option<SocketAddr>,
    upstream: Option<HttpComplexValue>
}

#[derive(Clone)]
pub struct ProxyContext {
    keepalive: usize,
    max_active: usize,
    proxy_timeout: Option<Duration>,
    keepalive_timeout: Option<Duration>,
    keepalive_requests: Option<u64>,
    primary: ProxyPass,
    backup: ProxyPass
}

impl Default for ProxyContext {
    fn default() -> ProxyContext {
        ProxyContext {
            keepalive: 0,
            max_active: std::usize::MAX,
            proxy_timeout: None,
            keepalive_timeout: None,
            keepalive_requests: None,
            primary: ProxyPass::default(),
            backup: ProxyPass::default()
        }
    }
}

pub struct Proxy {
}

impl Plugin for Proxy {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::ROUTE, "proxy.keepalive", |proxy: &mut ProxyContext, keepalive: usize| {
            proxy.keepalive = keepalive;
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "proxy.max_active", |proxy: &mut ProxyContext, max_active: usize| {
            proxy.max_active = max_active;
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "proxy.proxy_timeout", |proxy: &mut ProxyContext, proxy_timeout: Duration| {
            proxy.proxy_timeout = Some(proxy_timeout);
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "proxy.keepalive_timeout", |proxy: &mut ProxyContext, keepalive_timeout: Duration| {
            proxy.keepalive_timeout = Some(keepalive_timeout);
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "proxy.keepalive_requests", |proxy: &mut ProxyContext, keepalive_requests: u64| {
            proxy.keepalive_requests = Some(keepalive_requests);
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "proxy.pass", |proxy: &mut ProxyContext, pass: String| {
            match get_addr(&pass) {
                Ok(addr) => proxy.primary.pass = Some(addr),
                _ => proxy.primary.upstream = Some(Variable::complex(&pass))
            }
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "proxy.backup", |proxy: &mut ProxyContext, pass: String| {
            match get_addr(&pass) {
                Ok(addr) => proxy.backup.pass = Some(addr),
                _ => proxy.backup.upstream = Some(Variable::complex(&pass))
            }
            Ok(None)
        })?;

        add_block!(Context::ROUTE, "proxy", |context, pass: String| {
            match context.get_mut::<ProxyContext>() {
                Some(proxy) => {
                    // exit
                    let proxy = std::mem::take(proxy);
                    let upstream_module = HttpModule::get_plugin::<HttpUpstream>();

                    let get = |u: &ProxyPass| -> Result<Option<Arc<Upstream>>, CoreError> {
                        match u.upstream {
                            Some(_) => Ok(None),
                            None => {
                                let addr = match u.pass {
                                    Some(addr) => addr,
                                    None => return throw!("'pass' is not defined")
                                };
                                let mut upstream = Upstream::new(Box::new(RoundRobin::new()),
                                                                &addr.to_string(),
                                                                0,
                                                                0,
                                                                proxy.proxy_timeout,
                                                                proxy.keepalive_timeout,
                                                                proxy.keepalive_requests);
                                upstream.add_primary(addr, proxy.keepalive, proxy.max_active);
                                Ok(Some(Arc::new(upstream)))
                            }
                        }
                    };

                    let primary = get(&proxy.primary)?;
                    let backup = get(&proxy.backup).unwrap_or(None);

                    let connect = move |r: &HttpRequest| -> Result<Peer, CoreError> {
                        match match &primary {
                            None => match &proxy.primary.upstream {
                                Some(upstream) => {
                                    match upstream_module.connect(&r.expand(&upstream), proxy.proxy_timeout) {
                                        Ok(peer) => Ok(peer),
                                        Err(err) if proxy.backup.pass.is_none() && proxy.backup.upstream.is_none() => {
                                            return throw!(err)
                                        },
                                        err => err
                                    }
                                },
                                None => unreachable!()
                            },
                            Some(primary) => primary.connect(proxy.proxy_timeout)
                        } {
                            Ok(peer) => Ok(peer),
                            _ => {
                                match &backup {
                                    None => match &proxy.backup.upstream {
                                        Some(upstream) => upstream_module.connect(&r.expand(&upstream), proxy.proxy_timeout),
                                        None => unreachable!()
                                    },
                                    Some(backup) => backup.connect(proxy.proxy_timeout)
                                }
                            }
                        }
                    };

                    let bad_gateway = |resp: &mut HttpResponse| -> FlushResult {
                        resp.send(HttpStatus::BAD_GATEWAY, "text/plain", Some(b"Bad gateway"));
                        Ok(Flush::DECLINED)
                    };

                    context.parent().unwrap()
                           .get_mut::<RouteContext>()
                           .map(|route|
                    {
                        route.content = Some(ContentHandler::new(move |r| -> HttpResponse {
                            HttpResponse::with_status(r, HttpStatus::UNDEFINED)
                        }));

                        route.flush.push_back(FlushHandler::new(move |resp: &mut HttpResponse| -> FlushResult {
                            loop {
                                let mut context = match resp.take_context::<HttpProxyContext>("proxy") {
                                    Some(context) => context,
                                    None => match connect(resp.get_request()) {
                                        Ok(peer) => {
                                            let upstream_addr = peer.remote_addr();
                                            let upstream_name = peer.upstream();
                                            add_var_lazy!(resp, "upstream_name", move |_| upstream_name);
                                            add_var_lazy!(resp, "upstream_addr", move |_| upstream_addr);
                                            HttpProxyContext::new(peer)
                                        },
                                        Err(err) => {
                                            log_http_error!(resp, "error", err);
                                            return bad_gateway(resp);
                                        }
                                    }
                                };

                                let res = context.proxy(resp);

                                match res {
                                    Ok(Flush::READ_MORE(_)) | Ok(Flush::WRITE_MORE(_)) | Ok(Flush::READ_WRITE_MORE(_)) => {
                                        resp.set_context("proxy", context);
                                        return res;
                                    },
                                    Ok(Flush::OK(Some(peer))) => {
                                        let upstream_response_time = context.timer.elapsed().as_millis();
                                        let status = resp.status();
                                        add_var_lazy!(resp, "upstream_response_time", move |_| upstream_response_time);
                                        add_var_lazy!(resp, "upstream_status", move |_| status);
                                        return Ok(Flush::OK(Some(peer)));
                                    },
                                    Err(err) if context.state < HttpProxyState::st_protocol_end => {
                                        log_http_error!(resp, "error", err);
                                        context.peer.release();
                                        context.client.reset();
                                        /* try other server */
                                    },
                                    _ => return res
                                }
                            }
                        }));
                        Some(route)
                    }).unwrap();

                    Ok(None)
                },
                None => {
                    // enter
                    let mut proxy = ProxyContext::default();
                    proxy.keepalive = 10;
                    if pass.len() != 0 {
                        match get_addr(&pass) {
                            Ok(addr) => proxy.primary.pass = Some(addr),
                            _ => proxy.primary.upstream = Some(Variable::complex(&pass))
                        }
                    }
                    Ok(Some(CommandContext::new(proxy)))
                }
            }
        })?;

        Ok(OK)
    }
}

impl Proxy {
    pub fn new() -> Proxy {
        Proxy {}
    }
}

fn get_addr(addr: &str) -> Result<SocketAddr, CoreError> {
    match addr.parse() {
        Ok(addr) => Ok(addr),
        Err(err) => {
            throw!("Failed to parse bind address: {}", err)
        }
    }
}
