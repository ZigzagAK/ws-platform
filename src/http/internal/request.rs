/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use percent_encoding::{ percent_decode, utf8_percent_encode, NON_ALPHANUMERIC };
use chrono::prelude::*;
use std::time::Instant;

use crate::client_context::ClientContext;
use crate::http::error::HttpResult;
use crate::http::*;
use crate::keyval::Key;
use crate::http::{ HttpMethod, HttpProtocol };

const CR: u8 = 0x0D;
const LF: u8 = 0x0A;

#[derive(PartialEq, PartialOrd)]
#[allow(non_camel_case_types)]
enum HttpParseState {
    st_unparsed = 0,
    st_method,
    st_method_end,
    st_uri,
    st_uri_end,
    st_query,
    st_query_end,
    st_protocol,
    st_protocol_end,
    st_headers,
    st_headers_end,
    st_body,
    st_parsed
}

struct HttpRequestParseContext {
    state: HttpParseState,
    method: Vec<u8>,
    uri: Vec<u8>,
    query_string: Vec<u8>,
    protocol: Vec<u8>,
    key: Option<Vec<u8>>,
    val: Option<Vec<u8>>,
    expect_100_continue: bool
}

pub (crate) struct HttpRequest {
    // Client context

    pub client: ClientContext,

    // parse temporary context

    context: HttpRequestParseContext,

    // times

    pub start: DateTime<Utc>,
    pub timer: Instant,

    // parsed data

    pub content_length: Option<usize>,
    pub method: HttpMethod,
    pub protocol: HttpProtocol,
    pub host: String,
    pub request_uri: String,
    pub uri: String,
    pub query_string: String,
    pub vars: HttpVariables,
    pub args: HttpQuery,
    pub headers: HttpHeaders,
    pub body: Option<Vec<u8>>,

    // filters

    pub header_filter: LinkedList<HeaderFilterHandler>,
    pub body_filter: LinkedList<BodyFilterHandler>,
    pub flush: LinkedList<FlushHandler>,
    pub log: LinkedList<LogHandler>
}

impl std::fmt::Display for HttpProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            HttpProtocol::HTTP10 => write!(f, "1.0"),
            HttpProtocol::HTTP11 => write!(f, "1.1")
        }
    }
}

impl From<String> for HttpMethod {
    fn from(method: String) -> Self {
        match method.as_str() {
            "GET" => HttpMethod::GET,
            "HEAD" => HttpMethod::HEAD,
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "OPTIONS" => HttpMethod::OPTIONS,
            "MKCOL" => HttpMethod::MKCOL,
            "COPY" => HttpMethod::COPY,
            "MOVE" => HttpMethod::MOVE,
            "PROPFIND" => HttpMethod::PROPFIND,
            "PROPPATCH" => HttpMethod::PROPPATCH,
            "LOCK" => HttpMethod::LOCK,
            "UNLOCK" => HttpMethod::UNLOCK,
            "PATCH" => HttpMethod::PATCH,
            "TRACE" => HttpMethod::TRACE, 
            _ => HttpMethod::UNSUPPORTED
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            HttpMethod::UNSUPPORTED => write!(f, "UNSUPPORTED"),
            HttpMethod::GET => write!(f, "GET"),
            HttpMethod::HEAD => write!(f, "HEAD"),
            HttpMethod::POST => write!(f, "POST"),
            HttpMethod::PUT => write!(f, "PUT"),
            HttpMethod::DELETE => write!(f, "DELETE"),
            HttpMethod::OPTIONS => write!(f, "OPTIONS"),
            HttpMethod::MKCOL => write!(f, "MKCOL"),
            HttpMethod::COPY => write!(f, "COPY"),
            HttpMethod::MOVE => write!(f, "MOVE"),
            HttpMethod::PROPFIND => write!(f, "PROPFIND"),
            HttpMethod::PROPPATCH => write!(f, "PROPPATCH"),
            HttpMethod::LOCK => write!(f, "LOCK"),
            HttpMethod::UNLOCK => write!(f, "UNLOCK"),
            HttpMethod::PATCH => write!(f, "PATCH"),
            HttpMethod::TRACE => write!(f, "TRACE")
        }
    }
}

impl HttpRequest {
    pub fn new(client: ClientContext) -> HttpRequest {
        let host = format!("{}:{}", client.server_addr.ip(), client.server_addr.port());
        HttpRequest {
            context: HttpRequestParseContext {
                state: HttpParseState::st_unparsed,
                method: Vec::with_capacity(16),
                uri: Vec::with_capacity(128),
                query_string: Vec::with_capacity(128),
                protocol: Vec::with_capacity(8),
                key: Some(Vec::with_capacity(16)),
                val: None,
                expect_100_continue: false
            },
            start: Utc::now(),
            timer: Instant::now(),
            content_length: None,
            method: HttpMethod::UNSUPPORTED,
            protocol: HttpProtocol::HTTP10,
            host: host,
            uri: String::new(),
            request_uri: String::new(),
            query_string: String::new(),
            vars: KeyVal::default(),
            args: KeyVal::default(),
            headers: KeyVal::default(),
            body: None,
            client: client,
            header_filter: LinkedList::new(),
            body_filter: LinkedList::new(),
            flush: LinkedList::new(),
            log: LinkedList::new(),
        }
    }

    pub fn add_flush(&mut self, h: FlushHandler) {
        self.flush.push_back(h)
    }

    pub fn add_header_filter(&mut self, h: HeaderFilterHandler) {
        self.header_filter.push_back(h)
    }

    pub fn add_body_filter(&mut self, h: BodyFilterHandler) {
        self.body_filter.push_back(h)
    }

    pub fn add_log(&mut self, h: LogHandler) {
        self.log.push_back(h)
    }

    pub fn is_mailformed(this: &crate::http::HttpRequest) -> bool {
        this.inner.context.state < HttpParseState::st_parsed
    }

    pub fn parse(this: &mut crate::http::HttpRequest) -> HttpResult {
        match HttpRequest::parse_request_line(this)? {
            OK => match HttpRequest::parse_headers(this)? {
                OK => {
                    if this.inner.context.expect_100_continue {
                        this.inner.client.write(b"HTTP/1.1 100 Continue\r\ncontent-length: 0\r\n\r\n");
                        this.inner.client.flush().or_else(|err| http_fatal!(err.what()))?;
                        this.inner.context.expect_100_continue = false;
                        return Ok(AGAIN);
                    }
                    HttpRequest::read_body(this)
                },
                code => Ok(code)
            },
            code => Ok(code)
        }
    }

    pub fn parse_request_line(this: &mut crate::http::HttpRequest) -> HttpResult {
        match this.inner.parse_method()? {
            OK => match this.inner.parse_uri()? {
                OK => match this.inner.parse_args()? {
                    OK => this.inner.parse_protocol(),
                    code => Ok(code)
                },
                code => Ok(code)
            },
            code => Ok(code)
        }
    }

    fn parse_method(&mut self) -> HttpResult {
        let client = &mut self.client;

        if self.context.state > HttpParseState::st_method {
            return Ok(OK)
        }

        self.context.state = HttpParseState::st_method;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    b' ' => {
                        self.context.state = HttpParseState::st_method_end;
                        self.method = match &self.context.method[..] {
                            b"GET" => HttpMethod::GET,
                            b"HEAD" => HttpMethod::HEAD,
                            b"POST" => HttpMethod::POST,
                            b"PUT" => HttpMethod::PUT,
                            b"DELETE" => HttpMethod::DELETE,
                            b"OPTIONS" => HttpMethod::OPTIONS,
                            b"MKCOL" => HttpMethod::MKCOL,
                            b"COPY" => HttpMethod::COPY,
                            b"MOVE" => HttpMethod::MOVE,
                            b"PROPFIND" => HttpMethod::PROPFIND,
                            b"PROPPATCH" => HttpMethod::PROPPATCH,
                            b"LOCK" => HttpMethod::LOCK,
                            b"UNLOCK" => HttpMethod::UNLOCK,
                            b"PATCH" => HttpMethod::PATCH,
                            b"TRACE" => HttpMethod::TRACE,
                            _ => return http_fatal!("Unsupported method")
                        };
                        return Ok(OK);
                    },
                    c => self.context.method.push(c)
                }
            }

            match client.read() {
                Ok(OK)
                    => continue,
                Ok(AGAIN)
                    => return Ok(AGAIN),
                Ok(DECLINED) if client.buf.len() == 0
                    => return Ok(DECLINED),
                Ok(DECLINED)
                    => return http_fatal!("Client closed connection on read request line"),
                Err(err)
                    => return http_fatal!(err.what())
            }
        }
    }

    fn parse_protocol(&mut self) -> HttpResult {
        let client = &mut self.client;

        if self.context.state > HttpParseState::st_protocol {
            return Ok(OK)
        }

        self.context.state = HttpParseState::st_protocol;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    CR => { /* skip */ },
                    LF => {
                        self.context.state = HttpParseState::st_protocol_end;
                        self.protocol = match &self.context.protocol[..] {
                            b"HTTP/1.0" => HttpProtocol::HTTP10,
                            b"HTTP/1.1" => HttpProtocol::HTTP11,
                            _ => return http_throw!("Unsupported protocol version")
                        };
                        return Ok(OK);
                    },
                    c => self.context.protocol.push(c)
                }
            }
            read_more!(client, "Client has closed connection on read request line");
        }
    }

    fn url_decode(s: &[u8]) -> String {
        String::from(percent_decode(&s).decode_utf8_lossy())
    }

    fn url_encode(s: &str) -> String {
        utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
    }

    pub fn format_args(&self) -> String {
        let mut args = Vec::with_capacity(self.args.len());
        self.args.iter().for_each(|(k,v)| {
            v.iter().for_each(|v| {
                args.push(format!("{}={}", k, HttpRequest::url_encode(v)));
            })
        });
        args.join("&")
    }

    fn parse_uri(&mut self) -> HttpResult {
        let client = &mut self.client;

        if self.context.state > HttpParseState::st_uri {
            return Ok(OK)
        }

        self.context.state = HttpParseState::st_uri;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    b'?' => {
                        self.uri = String::from_utf8_lossy(&self.context.uri).to_string();
                        self.context.state = HttpParseState::st_uri_end;
                        return Ok(OK);
                    },
                    b' ' => {
                        self.uri = String::from_utf8_lossy(&self.context.uri).to_string();
                        self.request_uri = self.uri.clone();
                        self.context.state = HttpParseState::st_query_end;
                        return Ok(OK);
                    },
                    c => self.context.uri.push(c)
                }
            }
            read_more!(client, "Client has closed connection on read request line");
        }
    }

    fn parse_args(&mut self) -> HttpResult {
        let client = &mut self.client;

        if self.context.state > HttpParseState::st_query {
            return Ok(OK)
        }

        self.context.state = HttpParseState::st_query;

        loop {
            while !client.buf.end() {
                match client.buf.getc() {
                    b'=' => {
                        self.args = KeyVal::default();
                        self.context.val = Some(Vec::with_capacity(16));
                    },
                    b' ' => {
                        if let Some(k) = &self.context.key {
                            match &self.context.val {
                                Some(v) => {
                                    let k = HttpRequest::url_decode(&k);
                                    let ll = self.args.entry(Key::from(k)).or_default();
                                    ll.push_back(HttpRequest::url_decode(&v));
                                    self.context.state = HttpParseState::st_query_end;
                                    self.context.key = Some(Vec::with_capacity(64));
                                    self.context.val = None;
                                    self.query_string = String::from_utf8_lossy(&self.context.query_string).to_string();
                                    self.request_uri = format!("{}?{}", self.uri, self.query_string);
                                    return Ok(OK);
                                },
                                None => {
                                    // No args
                                    self.context.state = HttpParseState::st_query_end;
                                    self.context.key = Some(Vec::with_capacity(64));
                                    self.context.val = None;
                                    self.request_uri = self.uri.clone();
                                    return Ok(OK);
                                }
                            }
                        }
                        return http_throw!("Invalid query string");
                    },
                    b'&' => {
                        if let Some(k) = &self.context.key {
                            if let Some(v) = &self.context.val {
                                let key = HttpRequest::url_decode(&k);
                                let ll = self.args.entry(Key::from(key)).or_default();
                                ll.push_back(HttpRequest::url_decode(&v));
                                self.context.key = Some(Vec::with_capacity(16));
                                self.context.val = None;
                                continue;
                            }
                        }
                        return http_throw!("Invalid query string");
                    },
                    c => {
                        if let Some(ref mut v) = &mut self.context.val {
                            v.push(c);
                        } else if let Some(ref mut k) = &mut self.context.key {
                            assert!(self.context.val.is_none());
                            k.push(c);
                        }
                        self.context.query_string.push(c);
                    }
                }
            }
            read_more!(client, "Client has closed connection on read request line");
        }
    }

    pub fn parse_headers(this: &mut crate::http::HttpRequest) -> HttpResult {
        let client = &mut this.inner.client;

        if this.inner.context.state > HttpParseState::st_headers {
            return Ok(OK)
        }

        this.inner.context.state = HttpParseState::st_headers;

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
                            this.inner.context.state = HttpParseState::st_headers_end;
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

                        if let Some(k) = &this.inner.context.key {
                            if let Some(v) = &this.inner.context.val {
                                let name = Key::from(unsafe { std::str::from_utf8_unchecked(&k) }.trim());
                                let value = unsafe { std::str::from_utf8_unchecked(&v) }.trim();
                                match name.to_ascii_lowercase().as_str() {
                                    "content-length" => {
                                        match value.parse::<usize>() {
                                            Ok(len) => {
                                                this.inner.content_length = Some(len)
                                            },
                                            Err(_) => return http_throw!("Invalid header line")
                                        }
                                    },
                                    "expect" if value.to_ascii_lowercase() == "100-continue" => {
                                        this.inner.context.expect_100_continue = true;
                                    },
                                    "host" => this.inner.host = value.to_string(),
                                    _ => { /* void */ }
                                }
                                let ll = this.inner.headers.entry(Key::from(name)).or_default();
                                ll.push_back(value.to_string());
                                last = CR;
                                this.inner.context.key = Some(Vec::with_capacity(64));
                                this.inner.context.val = None;
                                continue;
                            }
                        }

                        return http_throw!("Invalid header line");
                    },
                    b':' => {
                        if let Some(ref mut v) = &mut this.inner.context.val {
                            v.push(b':');
                        } else {
                            this.inner.context.val = Some(Vec::with_capacity(64));
                        }
                    },
                    c => {
                        if let Some(ref mut v) = &mut this.inner.context.val {
                            v.push(c);
                        } else if let Some(ref mut k) = &mut this.inner.context.key {
                            assert!(this.inner.context.val.is_none());
                            k.push(c);
                        }
                        last = c;
                    }
                }
            }
            read_more!(client, "Client has closed connection on read headers");
        }
    }

    pub fn read_body(this: &mut crate::http::HttpRequest) -> HttpResult {
        if this.inner.context.state > HttpParseState::st_body {
            return Ok(OK)
        }

        this.inner.context.state = HttpParseState::st_body;

        if let Some(len) = this.inner.content_length {
            if len > 0 {
                loop {
                    match &mut this.inner.body {
                        None => this.inner.body = Some(Vec::from(this.inner.client.buf.tail())),
                        Some(ref mut body) => {
                            if body.len() == len {
                                break;
                            }
                            match this.inner.client.read() {
                                Ok(OK) => body.extend_from_slice(this.inner.client.buf.tail()),
                                Ok(AGAIN) => return Ok(AGAIN),
                                Err(err) => return http_fatal!(err.what()),
                                Ok(DECLINED) => return http_fatal!("Client has closed connection on read body")
                            }
                        }
                    }
                }
            }
        }

        this.inner.context.state = HttpParseState::st_parsed;

        Ok(OK)
    }
}
