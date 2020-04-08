/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::fs::File;
use std::io::{ ErrorKind, prelude::* };
use std::collections::HashMap;
use regex::Regex;
use std::mem::take;

use crate::http::error::HttpResult;
use crate::error::{ CoreResult, FlushResult, Flush };
use crate::http::*;
use crate::http::{ HttpStatus, HttpProtocol };

const CRLF: &[u8] = &[ 0x0d, 0x0a ];

lazy_static! {
    static ref MIME: HashMap<&'static str, &'static str> = {
        let mut map = HashMap::new();

        map.insert("html", "text/html");
        map.insert("htm", "text/html");
        map.insert("shtml", "text/html");
        map.insert("css", "text/css");
        map.insert("xml", "text/xml");
        map.insert("gif", "image/gif");
        map.insert("jpeg", "image/jpeg");
        map.insert("jpg", "image/jpeg");
        map.insert("js", "application/javascript");
        map.insert("atom", "application/atom+xml");
        map.insert("rss", "application/rss+xml");

        map.insert("mml", "text/mathml");
        map.insert("txt", "text/plain");
        map.insert("jad", "text/vnd.sun.j2me.app-descriptor");
        map.insert("wml", "text/vnd.wap.wml");
        map.insert("htc", "text/x-component");

        map.insert("png", "image/png");
        map.insert("svg", "image/svg+xml");
        map.insert("svgz", "image/svg+xml");
        map.insert("tif", "image/tiff");
        map.insert("tiff", "image/tiff");
        map.insert("wbmp", "image/vnd.wap.wbmp");
        map.insert("webp", "image/webp");
        map.insert("ico", "image/x-icon");
        map.insert("jng", "image/x-jng");
        map.insert("bmp", "image/x-ms-bmp");

        map.insert("woff", "font/woff");
        map.insert("woff2", "font/woff2");

        map.insert("jar", "application/java-archive");
        map.insert("war", "application/java-archive");
        map.insert("ear", "application/java-archive");
        map.insert("json", "application/json");
        map.insert("hqx", "application/mac-binhex40");
        map.insert("doc", "application/msword");
        map.insert("pdf", "application/pdf");
        map.insert("ps", "application/postscript");
        map.insert("eps", "application/postscript");
        map.insert("ai", "application/postscript");
        map.insert("rtf", "application/rtf");
        map.insert("m3u8", "application/vnd.apple.mpegurl");
        map.insert("kml", "application/vnd.google-earth.kml+xml");
        map.insert("kmz", "application/vnd.google-earth.kmz");
        map.insert("xls", "application/vnd.ms-excel");
        map.insert("eot", "application/vnd.ms-fontobject");
        map.insert("ppt", "application/vnd.ms-powerpoint");
        map.insert("odg", "application/vnd.oasis.opendocument.graphics");
        map.insert("odp", "application/vnd.oasis.opendocument.presentation");
        map.insert("ods", "application/vnd.oasis.opendocument.spreadsheet");
        map.insert("odt", "application/vnd.oasis.opendocument.text");
        map.insert("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation");
        map.insert("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        map.insert("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document");
        map.insert("wmlc", "application/vnd.wap.wmlc");
        map.insert("7z", "application/x-7z-compressed");
        map.insert("cco", "application/x-cocoa");
        map.insert("jardiff", "application/x-java-archive-diff");
        map.insert("jnlp", "application/x-java-jnlp-file");
        map.insert("run", "application/x-makeself");
        map.insert("pl", "application/x-perl");
        map.insert("pm", "application/x-perl");
        map.insert("prc", "application/x-pilot");
        map.insert("pdb", "application/x-pilot");
        map.insert("rar", "application/x-rar-compressed");
        map.insert("rpm", "application/x-redhat-package-manager");
        map.insert("sea", "application/x-sea");
        map.insert("swf", "application/x-shockwave-flash");
        map.insert("sit", "application/x-stuffit");
        map.insert("tcl", "application/x-tcl");
        map.insert("tk", "application/x-tcl");
        map.insert("der", "application/x-x509-ca-cert");
        map.insert("pem", "application/x-x509-ca-cert");
        map.insert("crt", "application/x-x509-ca-cert");
        map.insert("xpi", "application/x-xpinstall");
        map.insert("xhtml", "application/xhtml+xml");
        map.insert("xspf", "application/xspf+xml");
        map.insert("zip", "application/zip");

        map.insert("bin", "application/octet-stream");
        map.insert("exe", "application/octet-stream");
        map.insert("dll", "application/octet-stream");
        map.insert("deb", "application/octet-stream");
        map.insert("dmg", "application/octet-stream");
        map.insert("iso", "application/octet-stream");
        map.insert("img", "application/octet-stream");
        map.insert("msi", "application/octet-stream");
        map.insert("msp", "application/octet-stream");
        map.insert("msm", "application/octet-stream");

        map.insert("mid", "audio/midi");
        map.insert("midi", "audio/midi");
        map.insert("kar", "audio/midi");
        map.insert("mp3", "audio/mpeg");
        map.insert("ogg", "audio/ogg");
        map.insert("m4a", "audio/x-m4a");
        map.insert("ra", "audio/x-realaudio");

        map.insert("3gpp", "video/3gpp");
        map.insert("3gp", "video/3gpp");
        map.insert("ts", "video/mp2t");
        map.insert("mp4", "video/mp4");
        map.insert("mpeg", "video/mpeg");
        map.insert("mpg", "video/mpeg");
        map.insert("mov", "video/quicktime");
        map.insert("webm", "video/webm");
        map.insert("flv", "video/x-flv");
        map.insert("m4v", "video/x-m4v");
        map.insert("mng", "video/x-mng");
        map.insert("asx", "video/x-ms-asf");
        map.insert("asf", "video/x-ms-asf");
        map.insert("wmv", "video/x-ms-wmv");
        map.insert("avi", "video/x-msvideo");

        map
    };
}

macro_rules! headers_already_sent {
    ($f:literal) => { log_error!("warn", "$f: Headers already sent") }
}

pub (crate) struct HttpResponse {
    pub protocol: HttpProtocol,
    pub status: HttpStatus,
    pub headers: HttpHeaders,
    pub content_length: Option<usize>,
    pub body: Option<Vec<u8>>,
    pub transfer_encoding: TransferEncoding,
    file: Option<File>,
    closed: bool,
    headers_sent: bool,
    body_sent: bool
}

impl From<i64> for HttpStatus {
    fn from(status: i64) -> HttpStatus {
        match status {
            100 => HttpStatus::CONTINUE,
            101 => HttpStatus::SWITCHING_PROTOCOLS,
            200 => HttpStatus::OK,
            201 => HttpStatus::CREATED,
            202 => HttpStatus::ACCEPTED,
            204 => HttpStatus::NO_CONTENT,
            206 => HttpStatus::PARTIAL_CONTENT,
            300 => HttpStatus::SPECIAL_RESPONSE,
            301 => HttpStatus::MOVED_PERMANENTLY,
            302 => HttpStatus::MOVED_TEMPORARILY,
            303 => HttpStatus::SEE_OTHER,
            304 => HttpStatus::NOT_MODIFIED,
            307 => HttpStatus::TEMPORARY_REDIRECT,
            308 => HttpStatus::PERMANENT_REDIRECT,
            400 => HttpStatus::BAD_REQUEST,
            401 => HttpStatus::UNAUTHORIZED,
            402 => HttpStatus::PAYMENT_REQUIRED,
            403 => HttpStatus::FORBIDDEN,
            404 => HttpStatus::NOT_FOUND,
            405 => HttpStatus::NOT_ALLOWED,
            406 => HttpStatus::NOT_ACCEPTABLE,
            408 => HttpStatus::REQUEST_TIMEOUT,
            409 => HttpStatus::CONFLICT,
            410 => HttpStatus::GONE,
            426 => HttpStatus::UPGRADE_REQUIRED,
            429 => HttpStatus::TOO_MANY_REQUESTS,
            444 => HttpStatus::CLOSE,
            451 => HttpStatus::ILLEGAL,
            500 => HttpStatus::INTERNAL_SERVER_ERROR,
            501 => HttpStatus::METHOD_NOT_IMPLEMENTED,
            502 => HttpStatus::BAD_GATEWAY,
            503 => HttpStatus::SERVICE_UNAVAILABLE,
            504 => HttpStatus::GATEWAY_TIMEOUT,
            505 => HttpStatus::VERSION_NOT_SUPPORTED,
            507 => HttpStatus::INSUFFICIENT_STORAGE,
            _   => HttpStatus::BAD_REQUEST
        }
    }
}

impl std::fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            HttpStatus::UNDEFINED => write!(f, "0 UNDEFINED"),
            HttpStatus::CONTINUE => write!(f, "100 CONTINUE"),
            HttpStatus::SWITCHING_PROTOCOLS => write!(f, "101 SWITCHING PROTOCOLS"),
            HttpStatus::OK => write!(f, "200 OK"),
            HttpStatus::CREATED => write!(f, "201 CREATED"),
            HttpStatus::ACCEPTED => write!(f, "202 ACCEPTED"),
            HttpStatus::NO_CONTENT => write!(f, "204 NO CONTENT"),
            HttpStatus::PARTIAL_CONTENT => write!(f, "206 PARTIAL CONTENT"),
            HttpStatus::SPECIAL_RESPONSE => write!(f, "300 SPECIAL RESPONSE"),
            HttpStatus::MOVED_PERMANENTLY => write!(f, "301 MOVED PERMANENTLY"),
            HttpStatus::MOVED_TEMPORARILY => write!(f, "302 MOVED TEMPORARILY"),
            HttpStatus::SEE_OTHER => write!(f, "303 SEE OTHER"),
            HttpStatus::NOT_MODIFIED => write!(f, "304 NOT MODIFIED"),
            HttpStatus::TEMPORARY_REDIRECT => write!(f, "307 TEMPORARY REDIRECT"),
            HttpStatus::PERMANENT_REDIRECT => write!(f, "308 PERMANENT REDIRECT"),
            HttpStatus::BAD_REQUEST => write!(f, "400 BAD REQUEST"),
            HttpStatus::UNAUTHORIZED => write!(f, "401 UNAUTHORIZED"),
            HttpStatus::PAYMENT_REQUIRED => write!(f, "402 PAYMENT REQUIRED"),
            HttpStatus::FORBIDDEN => write!(f, "403 FORBIDDEN"),
            HttpStatus::NOT_FOUND => write!(f, "404 NOT FOUND"),
            HttpStatus::NOT_ALLOWED => write!(f, "405 NOT ALLOWED"),
            HttpStatus::NOT_ACCEPTABLE => write!(f, "406 NOT ACCEPTABLE"),
            HttpStatus::REQUEST_TIMEOUT => write!(f, "408 REQUEST TIMEOUT"),
            HttpStatus::CONFLICT => write!(f, "409 CONFLICT"),
            HttpStatus::GONE => write!(f, "410 GONE"),
            HttpStatus::UPGRADE_REQUIRED => write!(f, "426 UPGRADE REQUIRED"),
            HttpStatus::TOO_MANY_REQUESTS => write!(f, "429 TOO MANY REQUESTS"),
            HttpStatus::CLOSE => write!(f, "444 CLOSE"),
            HttpStatus::ILLEGAL => write!(f, "451 ILLEGAL"),
            HttpStatus::INTERNAL_SERVER_ERROR => write!(f, "500 INTERNAL SERVER ERROR"),
            HttpStatus::METHOD_NOT_IMPLEMENTED => write!(f, "501 METHOD NOT IMPLEMENTED"),
            HttpStatus::BAD_GATEWAY => write!(f, "502 BAD GATEWAY"),
            HttpStatus::SERVICE_UNAVAILABLE => write!(f, "503 SERVICE UNAVAILABLE"),
            HttpStatus::GATEWAY_TIMEOUT => write!(f, "504 GATEWAY TIMEOUT"),
            HttpStatus::VERSION_NOT_SUPPORTED => write!(f, "505 VERSION NOT SUPPORTED"),
            HttpStatus::INSUFFICIENT_STORAGE => write!(f, "507 INSUFFICIENT STORAGE")
        }
    }
}

impl HttpResponse {
    pub fn new(request: &HttpRequest) -> HttpResponse {
        HttpResponse {
            headers_sent: false,
            body_sent: false,
            transfer_encoding: TransferEncoding(0),
            content_length: None,
            file: None,
            closed: request.is_mailformed(),
            status: HttpStatus::OK,
            protocol: request.protocol(),
            headers: HttpHeaders::default(),
            body: None
        }
    }

    pub fn with_status(request: &HttpRequest, status: HttpStatus) -> HttpResponse {
        let mut resp = HttpResponse::new(request);
        resp.status = status;
        resp
    }

    pub fn reset(this: &mut crate::http::HttpResponse) {
        if this.inner.headers_sent {
            return headers_already_sent!("reset");
        }

        this.inner.status = HttpStatus::OK;
        this.inner.transfer_encoding = TransferEncoding(0);
        this.inner.content_length = None;
        this.inner.body = None;
        this.inner.file = None;
        this.inner.headers.clear();
        this.inner.closed = false;

        this.context().reset();
    }

    pub fn send(this: &mut crate::http::HttpResponse, status: HttpStatus, content_type: &str, text: Option<&[u8]>) {
        HttpResponse::set_status(this, status);
        match text {
            Some(text) => {
                HttpResponse::set_content_type(this, content_type);
                HttpResponse::set_content_length(this, text.len());
                this.inner.body = Some(Vec::from(text));
            },
            None => {
                match status {
                    HttpStatus::NO_CONTENT => HttpResponse::send_no_content(this),
                    _ => HttpResponse::flush_headers(this)
                }
            }
        }
    }

    pub fn send_not_modified(this: &mut crate::http::HttpResponse) {
        if this.inner.headers_sent {
            return headers_already_sent!("send_not_modified");
        }

        HttpResponse::set_status(this, HttpStatus::NOT_MODIFIED);

        this.inner.content_length = None;
        this.inner.body = None;
        this.inner.file = None;
    }

    pub fn send_no_content(this: &mut crate::http::HttpResponse) {
        if this.inner.headers_sent {
            return headers_already_sent!("send_no_content");
        }

        HttpResponse::set_status(this, HttpStatus::NO_CONTENT);
        this.inner.content_length = Some(0);
    }

    pub fn set_status(this: &mut crate::http::HttpResponse, status: HttpStatus) {
        if this.inner.headers_sent {
            return headers_already_sent!("set_status");
        }

        this.inner.status = status;
    }

    pub fn replace_header(this: &mut crate::http::HttpResponse, name: &str, value: Option<&str>) {
        if this.inner.headers_sent {
            return headers_already_sent!("replace_header");
        }

        this.inner.headers.replace(name, value.map(|v| v.to_string()))
    }

    pub fn set_header(this: &mut crate::http::HttpResponse, name: &str, value: &str) {
        this.inner.headers.set(name, value.to_string())
    }

    pub fn add_header(this: &mut crate::http::HttpResponse, name: &str, value: &str) {
        if this.inner.headers_sent {
            return headers_already_sent!("set_header");
        }

        this.inner.headers.add(name, value.to_string())
    }

    pub fn remove_header(this: &mut crate::http::HttpResponse, name: &str) {
        this.inner.headers.remove(name);
    }

    pub fn set_content_type(this: &mut crate::http::HttpResponse, content_type: &str) {
        HttpResponse::set_header(this, "Content-Type", content_type)
    }

    pub fn set_content_length(this: &mut crate::http::HttpResponse, content_length: usize) {
        if this.inner.headers_sent {
            return headers_already_sent!("set_content_length");
        }

        this.inner.content_length = Some(content_length);
        this.inner.transfer_encoding.0 &= !TransferEncoding::CHUNKED;
    }

    pub fn send_body_chunk(this: &mut crate::http::HttpResponse, data: Option<&[u8]>) -> HttpResult {
        if this.inner.body_sent {
            return http_throw!("send_body_chunk: Body already sent");
        }

        HttpResponse::flush_headers(this);

        let mut body = match data {
            Some(data) => Some(Vec::from(data)),
            None => None
        };

        this.request.inner.body_filter.iter().for_each(|h| {
            body = h.handle(body.take())
        });

        if this.inner.transfer_encoding.is_chunked() {
            let chunk_size = match data {
                Some(text) => text.len(),
                None => 0
            };
            this.context().write_str(&format!("{:x}\r\n", chunk_size));
        }

        if let Some(body) = body {
            this.context().write(&body);
        }

        if this.inner.transfer_encoding.is_chunked() {
            this.context().write(CRLF);
        }

        Ok(OK)
    }

    pub fn send_file(this: &mut crate::http::HttpResponse, file: &str) -> HttpResult {
        fn mime(file: &str) -> &str {
            let re = Regex::new(r"\.([^.]+)$").unwrap();
            let caps = re.captures(file).unwrap();
            match caps.get(1) {
                Some(m) => match MIME.get(m.as_str()) {
                    Some(mimi_type) => mimi_type,
                    None => "text/html"
                },
                None => "text/html"
            }
        }

        HttpResponse::reset(this);

        let file = file.trim_start_matches("/");

        match std::fs::metadata(&file) {
            Ok(m) => {
                match File::open(&file) {
                    Ok(f) => {
                        HttpResponse::set_status(this, HttpStatus::OK);
                        HttpResponse::set_content_length(this, m.len() as usize);
                        HttpResponse::set_content_type(this, &mime(&file));
                        this.inner.file = Some(f);
                        return Ok(OK);
                    },
                    Err(err) => {
                        println!("Failed to open file '{}': {}", &file, err);
                    }
                };
            },
            Err(err) => {
                println!("Failed to obtain metadata for file '{}': {}", &file, err);
            }
        };

        HttpResponse::send(this, HttpStatus::NOT_FOUND, "text/plain", Some(b"Not found"));

        Ok(OK)
    }

    fn flush_headers(this: &mut crate::http::HttpResponse) {
        if this.inner.headers_sent {
            return;
        }

        HttpResponse::set_header(this, "Server", "WS-Platform/0.0.1");

        match this.inner.protocol {
            HttpProtocol::HTTP11 => {
                let connection = match this.request.headers().exact("connection") {
                    Some(connection) if connection.to_ascii_lowercase() == "close" => {
                        this.inner.closed = true;
                        "close"
                    },
                    _ => "keep-alive"
                };
                HttpResponse::set_header(this, "Connection", connection);
            },
            HttpProtocol::HTTP10 => {
                HttpResponse::set_header(this, "Connection", "close");
                this.inner.closed = true;
            }
        };

        for j in 0..2 {
            if j == 1 {
                this.inner.transfer_encoding = TransferEncoding::new(this.header_exact("Transfer-Encoding"));
                match this.inner.status {
                    HttpStatus::NOT_MODIFIED | HttpStatus::NO_CONTENT => {
                        this.inner.transfer_encoding.0 &= !TransferEncoding::CHUNKED;
                        HttpResponse::remove_header(this, "Content-Length");
                        this.inner.content_length = None;
                        this.inner.body = None;
                    },
                    _ => match this.inner.content_length {
                        Some(content_length) => {
                            this.inner.transfer_encoding.0 &= !TransferEncoding::CHUNKED;
                            HttpResponse::set_header(this, "Content-Length", &content_length.to_string());
                        },
                        None => {
                            match this.request.protocol() {
                                HttpProtocol::HTTP11 => this.inner.transfer_encoding.0 |= TransferEncoding::CHUNKED,
                                HttpProtocol::HTTP10 => this.inner.transfer_encoding.0 &= !TransferEncoding::CHUNKED
                            }
                            HttpResponse::remove_header(this, "Content-Length");
                        }
                    }
                }
                HttpResponse::replace_header(this, "Transfer-Encoding", this.inner.transfer_encoding.format().as_ref().map(|v| v.as_str()));
            }

            take(&mut this.request.inner.header_filter).into_iter().for_each(|h| {
                h.handle(this)
            });

            if j == 0 {
                match this.header_exact("Content-Length") {
                    Some(content_length) => {
                        if let Ok(content_length) = content_length.parse::<usize>() {
                            this.inner.content_length = Some(content_length);
                        }
                    },
                    None => {
                        if TransferEncoding::new(this.header_exact("Transfer-Encoding")).is_chunked() {
                            this.inner.content_length = None
                        }
                    }
                }
            }
        }

        let mut headers = Vec::with_capacity(4096);

        this.inner.headers.iter().for_each(|(key,ll)| {
            ll.iter().for_each(|v| {
                headers.extend_from_slice(format!("{}: {}\r\n", &key, &v).as_bytes());
            })
        });

        let status_line = format!("HTTP/{} {}\r\n", this.inner.protocol, this.inner.status);

        this.context().write_str(&status_line);
        this.context().write(&headers);
        this.context().write(CRLF);

        this.inner.headers_sent = true;
    }

    fn flush_body(this: &mut crate::http::HttpResponse) {
        if this.inner.body_sent {
            return;
        }

        this.inner.body = match this.inner.body.take() {
            Some(body) => {
                HttpResponse::set_content_length(this, body.len());
                HttpResponse::send_body_chunk(this, Some(&body)).unwrap();
                this.inner.body_sent = true;
                Some(body)
            },
            None => {
                HttpResponse::flush_headers(this);
                None
            }
        }
    }

    fn flush_file(this: &mut crate::http::HttpResponse) -> CoreResult {
        if let Some(ref mut file) = &mut this.inner.file {
            let mut b = [0u8; 16384];
            return match file.read(&mut b) {
                Ok(0) => {
                    this.inner.body_sent = true;
                    Ok(OK)
                }
                Ok(sz) => {
                    this.context().reset();
                    HttpResponse::send_body_chunk(this, Some(&b[..sz])).unwrap();
                    Ok(AGAIN)
                },
                Err(err) => if err.kind() == ErrorKind::Interrupted {
                    Ok(AGAIN)
                } else {
                    throw!("Failed to read file: {}", err)
                }
            }
        }
        Ok(OK)
    }

    pub fn flush(this: &mut crate::http::HttpResponse) -> FlushResult  {
        loop {
            match this.request.inner.flush.pop_front() {
                Some(h) => {
                    let res = h.handle(this)?;
                    match res {
                        Flush::AGAIN | Flush::READ_MORE(_) | Flush::WRITE_MORE(_) | Flush::READ_WRITE_MORE(_) => {
                            this.request.inner.flush.push_front(h);
                            return Ok(res);
                        },
                        Flush::OK(None) | Flush::DECLINED => {},
                        Flush::OK(_) => {
                            return Ok(res)
                        }
                    }
                },
                None => {
                    HttpResponse::flush_body(this);
                    break;
                }
            }
        }

        loop {
            return match this.context().flush()?.0 {
                AGAIN => Ok(Flush::AGAIN),
                OK => {
                    match HttpResponse::flush_file(this)? {
                        AGAIN => continue,
                        OK => Ok(match this.inner.closed {
                            false => Flush::OK(None),
                            true => Flush::DECLINED
                        }),
                        DECLINED => unreachable!()
                    }
                },
                DECLINED => unreachable!()
            }
        }
    }
}