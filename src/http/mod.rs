/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::any::Any;
use std::ops::Deref;
use std::collections::{ HashMap, LinkedList };
use std::mem::take;
use std::time::Duration;

use crate::module::*;
use crate::config::{ CommandContext, CommandContextType };
use crate::error::{ *, Code::* };
use crate::keyval::*;
use crate::handler::sync::Handler;
use crate::handler::sync::RefHandler;
use crate::client_context::ClientContext;
use crate::http::error::HttpResult;
use crate::variable::Variable;
use crate::config::{ Map, List };

pub struct HTTP;

impl ModuleType for HTTP {
    type Request = HttpRequest;
    type Response = HttpResponse;
    fn name() -> &'static str {
        "http"
    }
    fn root_context() -> Option<CommandContextType> {
        Some(CommandContext::new_default::<HttpContext>())
    }
}

pub type HttpMap = Map<HttpRequest>;
pub type HttpList = List<HttpRequest>;
pub type HttpComplexValue = Variable<HttpRequest>;

pub type HttpHeaders = KeyVal<String>;
pub type HttpQuery = KeyVal<String>;
pub type HttpVariables = KeyVal<HttpComplexValue>;

pub type HttpModule = GenericModule<HTTP>;

pub enum Context {
    MAIN,
    HTTP,
    WORKGROUP,
    SERVER,
    UPSTREAM,
    ROUTE
}

impl Deref for Context {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        match self {
            Context::MAIN => "root",
            Context::HTTP => "root.http",
            Context::WORKGROUP => "root.http.workgroups.workgroup",
            Context::SERVER  => "root.http.servers.server",
            Context::UPSTREAM => "root.http.upstreams.upstream",
            Context::ROUTE => "root.http.servers.server.routes.route"
        }
    }
}

#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum HttpMethod {
    UNSUPPORTED,
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    OPTIONS,
    MKCOL,
    COPY,
    MOVE,
    PROPFIND,
    PROPPATCH,
    LOCK,
    UNLOCK,
    PATCH,
    TRACE
}

#[derive(PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum HttpProtocol {
    HTTP10,
    HTTP11
}

#[derive(Clone, Copy, PartialEq)]
#[allow(non_camel_case_types)]
pub enum HttpStatus {
    UNDEFINED = 0,
    CONTINUE = 100,
    SWITCHING_PROTOCOLS = 101,
    OK = 200,
    CREATED = 201,
    ACCEPTED = 202,
    NO_CONTENT = 204,
    PARTIAL_CONTENT = 206,
    SPECIAL_RESPONSE = 300,
    MOVED_PERMANENTLY = 301,
    MOVED_TEMPORARILY = 302,
    SEE_OTHER = 303,
    NOT_MODIFIED = 304,
    TEMPORARY_REDIRECT = 307,
    PERMANENT_REDIRECT = 308,
    BAD_REQUEST = 400,
    UNAUTHORIZED = 401,
    PAYMENT_REQUIRED = 402,
    FORBIDDEN = 403,
    NOT_FOUND = 404,
    NOT_ALLOWED = 405,
    NOT_ACCEPTABLE = 406,
    REQUEST_TIMEOUT = 408,
    CONFLICT = 409,
    GONE = 410,
    UPGRADE_REQUIRED = 426,
    TOO_MANY_REQUESTS = 429,
    CLOSE = 444,
    ILLEGAL = 451,
    INTERNAL_SERVER_ERROR = 500,
    METHOD_NOT_IMPLEMENTED = 501,
    BAD_GATEWAY = 502,
    SERVICE_UNAVAILABLE = 503,
    GATEWAY_TIMEOUT = 504,
    VERSION_NOT_SUPPORTED = 505,
    INSUFFICIENT_STORAGE = 507
}

#[derive(Default)]
pub struct TransferEncoding(u16);

impl TransferEncoding {
    const CHUNKED: u16 = 1;
    const COMPRESS: u16 = 2;
    const DEFLATE: u16 = 4;
    const GZIP: u16 = 8;
    const IDENTITY: u16 = 16;

    pub fn new(h: Option<&String>) -> TransferEncoding {
        let mut te = TransferEncoding(0);
        match h {
            None => te,
            Some(a) => {
                a.split(",").collect::<Vec<&str>>().into_iter().for_each(|v| {
                    te.parse(v.trim());
                });
                te
            }
        }    
    }

    pub fn parse(&mut self, v: &str) {
        match v.as_bytes() {
            b"chunked" => self.0 |= TransferEncoding::CHUNKED,
            b"compress" => self.0 |= TransferEncoding::COMPRESS,
            b"deflate" => self.0 |= TransferEncoding::DEFLATE,
            b"gzip" => self.0 |= TransferEncoding::GZIP,
            b"identity" => self.0 |= TransferEncoding::IDENTITY,
            _ => { /* skipped */ }
        }
    }

    pub fn format(&self) -> Option<String> {
        match self.0 {
            0 => None,
            _ => Some(self.to_string())
        }
    }

    pub fn is_chunked(&self) -> bool {
        self.0 & TransferEncoding::CHUNKED != 0
    }

    pub fn is_compress(&self) -> bool {
        self.0 & TransferEncoding::COMPRESS != 0
    }

    pub fn is_deflate(&self) -> bool {
        self.0 & TransferEncoding::DEFLATE != 0
    }

    pub fn is_gzip(&self) -> bool {
        self.0 & TransferEncoding::GZIP != 0
    }

    pub fn is_identity(&self) -> bool {
        self.0 & TransferEncoding::IDENTITY != 0
    }

    pub fn is_some(&self) -> bool {
        self.0 != 0
    }
}

impl std::fmt::Display for TransferEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut te = Vec::with_capacity(3);

        if self.is_chunked() {
            te.push("chunked");
        }
    
        if self.is_compress() {
            te.push("compress");
        }

        if self.is_deflate() {
            te.push("deflate");
        }

        if self.is_gzip() {
            te.push("gzip");
        }

        if self.is_identity() {
            te.push("identity");
        }

        write!(f, "{}", te.join(", "))
    }
}

pub struct HttpRequest {
    context: HashMap<&'static str, Box<dyn Any + Send>>,
    error_log: Option<String>,
    inner: internal::HttpRequest
}

impl Request for HttpRequest {
    fn new(client: ClientContext) -> Self {
        HttpRequest {
            inner: internal::HttpRequest::new(client),
            error_log: None,
            context: HashMap::new()
        }
    }

    fn parse(&mut self) -> CoreResult {
        match internal::HttpRequest::parse(self) {
            Ok(code) => Ok(code),
            Err(err) if err.is_fatal() => throw!(err.what()),
            Err(err) => {
                if internal::HttpRequest::is_mailformed(self) {
                    return Ok(OK);
                }
                println!("{}", err.what());
                return Ok(OK);
            }
        }
    }

    fn context(&mut self) -> &mut ClientContext {
        &mut self.inner.client
    }

    fn const_context(&self) -> &ClientContext {
        &self.inner.client
    }

    fn close(self) -> ClientContext {
        self.inner.client
    }
}

impl HttpRequest {
    pub fn parse_request_line(&mut self) -> HttpResult {
        internal::HttpRequest::parse_request_line(self)
    }

    pub fn parse_headers(&mut self) -> HttpResult {
        internal::HttpRequest::parse_headers(self)
    }

    pub fn read_body(&mut self) -> HttpResult {
        internal::HttpRequest::read_body(self)
    }

    pub fn set_context<T: Send + 'static>(&mut self, module: &'static str, context: T) {
        self.context.insert(module, Box::new(context));
    }

    pub fn clear_context(&mut self, module: &'static str) {
        self.context.remove(module);
    }

    pub fn take_context<T: Send + 'static>(&mut self, module: &str) -> Option<T> {
        match self.context.remove(module) {
            Some(context) => match context.downcast::<T>() {
                Ok(context) => Some(*context),
                _ => None
            },
            None => None
        }
    }

    pub fn set_error_log(&mut self, error_log: &String) {
        self.error_log = Some(error_log.clone())
    }

    pub fn get_error_log(&self) -> &Option<String> {
        &self.error_log
    }

    pub fn request_start(&self) -> chrono::DateTime<chrono::Utc> {
        self.inner.start
    }

    pub fn request_time(&self) -> u64 {
        self.inner.timer.elapsed().as_millis() as u64
    }

    pub fn content_length(&self) -> Option<usize> {
        self.inner.content_length
    }

    pub fn method(&self) -> HttpMethod {
        self.inner.method
    }

    pub fn protocol(&self) -> HttpProtocol {
        self.inner.protocol
    }

    pub fn host(&self) -> &String {
        &self.inner.host
    }

    pub fn request_uri(&self) -> &String {
        &self.inner.request_uri
    }

    pub fn format_args(&self) -> String {
        self.inner.format_args()
    }

    pub fn rewrite(&mut self, uri: &String) {
        self.inner.uri = uri.clone()
    }

    pub fn uri(&self) -> &String {
        &self.inner.uri
    }

    pub fn query_string(&self) -> &String {
        &self.inner.query_string
    }

    pub fn add_var(&mut self, name: &str, value: Variable<HttpRequest>) {
        self.inner.vars.add(name, value)
    }

    pub fn vars(&self) -> &HttpVariables {
        &self.inner.vars
    }

    pub fn args(&self) -> &HttpQuery {
        &self.inner.args
    }

    pub fn headers(&self) -> &HttpHeaders {
        &self.inner.headers
    }

    pub fn vars_mut(&mut self) -> &mut HttpVariables {
        &mut self.inner.vars
    }

    pub fn args_mut(&mut self) -> &mut HttpQuery {
        &mut self.inner.args
    }

    pub fn headers_mut(&mut self) -> &mut HttpHeaders {
        &mut self.inner.headers
    }

    pub fn expand(&self, cv: &Variable<HttpRequest>) -> String {
        cv.expand_with(|var: &str| -> Option<String> {
            if var.starts_with("http_") {
                return self.inner.headers.exact(&var[5..]).map(|s| s.clone())
            }
            if var.starts_with("arg_") {
                return self.inner.args.exact(&var[4..]).map(|s| s.clone())
            }
            match self.inner.vars.exact(var) {
                Some(var) => Some(self.expand(var)),
                None => None
            }    
        }, self)
    }

    pub fn body(&self) -> Option<&[u8]> {
        match &self.inner.body {
            Some(body) => Some(body),
            None => None
        }
    }

    pub fn is_mailformed(&self) -> bool {
        internal::HttpRequest::is_mailformed(self)
    }

    pub fn add_flush(&mut self, h: FlushHandler) {
        self.inner.add_flush(h)
    }

    pub fn add_header_filter(&mut self, h: HeaderFilterHandler) {
        self.inner.add_header_filter(h)
    }

    pub fn add_body_filter(&mut self, h: BodyFilterHandler) {
        self.inner.add_body_filter(h)
    }

    pub fn add_log(&mut self, h: LogHandler) {
        self.inner.add_log(h)
    }
}

pub struct HttpResponse {
    request: HttpRequest,
    inner: internal::HttpResponse
}

impl Response for HttpResponse {

    type Request = HttpRequest;

    fn new(request: Self::Request) -> Self {
        HttpResponse {
            inner: internal::HttpResponse::new(&request),
            request: request
        }
    }

    fn flush(&mut self) -> FlushResult {
        internal::HttpResponse::flush(self)
    }

    fn get_request(&mut self) -> &mut Self::Request {
        &mut self.request
    }

    fn close(mut self) -> ClientContext {
        take(&mut self.request.inner.log).iter().for_each(|h| h.handle(&mut self));
        self.request.close()
    }
}

impl HttpResponse {
    fn with_status(request: HttpRequest, status: HttpStatus) -> HttpResponse {
        HttpResponse {
            inner: internal::HttpResponse::with_status(&request, status),
            request: request
        }
    }

    pub fn get_error_log(&self) -> &Option<String> {
        self.request.get_error_log()
    }

    pub fn set_context<T: Send + 'static>(&mut self, module: &'static str, context: T) {
        self.request.set_context::<T>(module, context)
    }

    pub fn clear_context(&mut self, module: &'static str) {
        self.request.clear_context(module);
    }

    pub fn take_context<T: Send + 'static>(&mut self, module: &str) -> Option<T> {
        self.request.take_context::<T>(module)
    }

    pub fn reset(&mut self) {
        internal::HttpResponse::reset(self)
    }

    pub fn send(&mut self, status: HttpStatus, content_type: &str, text: Option<&[u8]>) {
        internal::HttpResponse::send(self, status, content_type, text)
    }

    pub fn send_not_modified(&mut self) {
        internal::HttpResponse::send_not_modified(self)
    }

    pub fn send_no_content(&mut self) {
        internal::HttpResponse::send_no_content(self)
    }

    pub fn set_status(&mut self, status: HttpStatus) {
        internal::HttpResponse::set_status(self, status)
    }

    pub fn set_header(&mut self, name: &str, value: &str) {
        internal::HttpResponse::set_header(self, name, value)
    }

    pub fn add_header(&mut self, name: &str, value: &str) {
        internal::HttpResponse::add_header(self, name, value)
    }

    pub fn replace_header(&mut self, name: &str, value: Option<&str>) {
        internal::HttpResponse::replace_header(self, name, value)
    }

    pub fn remove_header(&mut self, name: &str) {
        internal::HttpResponse::remove_header(self, name)
    }

    pub fn set_content_length(&mut self, content_length: usize) {
        internal::HttpResponse::set_content_length(self, content_length);
    }

    pub fn set_content_type(&mut self, content_type: &str) {
        internal::HttpResponse::set_content_type(self, content_type)
    }

    pub fn body_len(&self) -> usize {
        match self.body() {
            Some(body) => body.len(),
            None => 0
        }
    }

    pub fn append_body(&mut self, chunk: &[u8]) {
        let len;

        self.inner.body = Some({
            let mut body = match self.inner.body.take() {
                Some(body) => body,
                None => Vec::with_capacity(match self.content_length() {
                    Some(content_length) => content_length,
                    None => chunk.len()
                })
            };
            body.extend_from_slice(chunk);
            len = body.len();
            body
        });

        if self.chunked() {
            return;
        }

        internal::HttpResponse::set_content_length(self, match self.inner.content_length {
            Some(content_length) if content_length >= len => content_length,
            _ => len
        })
    }

    pub fn set_body(&mut self, body: &[u8]) {
        self.set_content_length(body.len());
        self.inner.body = Some(Vec::from(body));
    }

    pub fn send_body_chunk(&mut self, text: Option<&[u8]>) -> HttpResult {
        internal::HttpResponse::send_body_chunk(self, text)
    }

    pub fn send_file(&mut self, file: &str) -> HttpResult {
        internal::HttpResponse::send_file(self, file)
    }

    pub fn set_chunked(&mut self) {
        self.inner.transfer_encoding.0 |= TransferEncoding::CHUNKED;
        self.inner.content_length = None;
    }

    pub fn chunked(&self) -> bool {
        self.inner.transfer_encoding.is_chunked()
    }

    pub fn protocol(&self) -> HttpProtocol {
        self.inner.protocol
    }

    pub fn status(&self) -> HttpStatus {
        self.inner.status
    }

    pub fn content_length(&self) -> Option<usize> {
        self.inner.content_length
    }

    pub fn headers(&mut self) -> &mut HttpHeaders {
        &mut self.inner.headers
    }

    pub fn header(&self, name: &str) -> Option<Value<'_, String>> {
        self.inner.headers.get(name)
    }

    pub fn header_exact(&self, name: &str) -> Option<&String> {
        self.inner.headers.exact(name)
    }

    pub fn body(&self) -> Option<&[u8]> {
        match &self.inner.body {
            Some(body) => Some(body),
            None => None
        }
    }

    pub fn expand(&self, cv: &Variable<HttpRequest>) -> String {
        cv.expand_with(|var: &str| -> Option<String> {
            if var.starts_with("http_") {
                return self.request.inner.headers.exact(&var[5..]).map(|s| s.clone())
            }
            if var.starts_with("arg_") {
                return self.request.inner.args.exact(&var[4..]).map(|s| s.clone())
            }
            if var.starts_with("sent_http_") {
                return self.inner.headers.exact(&var[10..]).map(|s| s.clone())
            }
            match self.request.inner.vars.exact(var) {
                Some(var) => Some(self.expand(var)),
                None => None
            }    
        }, &self.request)
    }

    pub fn add_var(&mut self, name: &str, value: Variable<HttpRequest>) {
        self.request.add_var(name, value)
    }

    pub fn add_header_filter(&mut self, h: HeaderFilterHandler) {
        self.request.add_header_filter(h)
    }

    pub fn add_body_filter(&mut self, h: BodyFilterHandler) {
        self.request.add_body_filter(h)
    }

    pub fn add_flush(&mut self, h: FlushHandler) {
        self.request.add_flush(h)
    }

    pub fn add_log(&mut self, h: LogHandler) {
        self.request.add_log(h)
    }
}

pub type SetVarHandler = RefHandler<HttpRequest, Code>;
pub type RewriteHandler = RefHandler<HttpRequest, Code>;
pub type AccessHandler = RefHandler<HttpRequest, Code>;
pub type ContentHandler = Handler<HttpRequest, HttpResponse>;
pub type HeaderFilterHandler = RefHandler<HttpResponse, ()>;
pub type BodyFilterHandler = Handler<Option<Vec<u8>>, Option<Vec<u8>>>;
pub type FlushHandler = RefHandler<HttpResponse, FlushResult>;
pub type LogHandler = RefHandler<HttpResponse, ()>;

#[derive(Clone, Default)]
pub struct HttpContext {
    pub setvar: LinkedList<SetVarHandler>,
    pub error_log: Option<String>
}

#[derive(Clone, Default)]
pub struct ServerContext {
    pub workgroup: String,
    pub bind: String,
    pub error_log: Option<String>,
    pub virtual_host: Option<String>,
    pub routes: Option<LinkedList<RouteContext>>,
    pub request_timeout: Option<Duration>,
    pub response_timeout: Option<Duration>,
    pub keepalive_timeout: Option<Duration>,
    pub keepalive_requests: u64,
    pub setvar: LinkedList<SetVarHandler>,
    pub rewrite: LinkedList<RewriteHandler>,
    pub access: LinkedList<AccessHandler>,
    pub header_filter: LinkedList<HeaderFilterHandler>,
    pub body_filter: LinkedList<BodyFilterHandler>,
    pub log: LinkedList<LogHandler>
}

#[derive(Clone, Default)]
pub struct RouteContext {
    pub host: Option<String>,
    pub pattern: String,
    pub method: Option<HttpMethod>,
    pub error_log: Option<String>,
    pub setvar: LinkedList<SetVarHandler>,
    pub rewrite: LinkedList<RewriteHandler>,
    pub access: LinkedList<AccessHandler>,
    pub content: Option<ContentHandler>,
    pub header_filter: LinkedList<HeaderFilterHandler>,
    pub body_filter: LinkedList<BodyFilterHandler>,
    pub flush: LinkedList<FlushHandler>,
    pub log: LinkedList<LogHandler>
}

#[macro_export]
macro_rules! register_http_plugin {
    ($name:ident) => {
        register_plugin!(HttpModule, $name);
    }
}

#[macro_use]
pub mod error;
pub mod routers;
pub mod server;
pub mod http_server_core;
pub mod plugins;
mod internal;