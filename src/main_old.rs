/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

extern crate web_server;
extern crate chrono;

use web_server::http::default_web_server::{ DefaultWebServer, RouteHandler };
use web_server::http::server::HttpServerHandler;
use web_server::http::response::{ HttpResponse, HttpStatus };
use web_server::http::request::HttpMethod;

use std::thread;
use chrono::Local;
use std::time::Duration;

use regex::Regex;

/*
    resp.send_status(HttpStatus::OK);
    for _ in 0..100 {
        resp.send_body_chunk(Some(b"111111111111"));
        resp.send_body_chunk(Some(b"22222222"));
        resp.send_body_chunk(Some(b"3333"));
    }
    resp.send_body_chunk(None);


    resp.send_no_content();

    resp.send_status(HttpStatus::OK);
    resp.send_content_length(8);
    resp.send_body_chunk(Some(b"1111"));
    resp.send_body_chunk(Some(b"2222"));

    resp.send(HttpStatus::OK, "text/plain", Some(b"Hello!"));
*/

fn main() {
    let mut http = DefaultWebServer::new(10, 4096).unwrap();

    let mut api = DefaultWebServer::new(10, 4096).unwrap();

    let default_api_handler = HttpServerHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);

        let re = Regex::new(r"\.([^.]+)$").unwrap();

        match re.is_match(&resp.r.uri) {
            true => {
                let uri = String::from(&resp.r.uri);
                resp.send_file(&uri);
            },
            false => {
                let body = if let Some(ref body) = &resp.r.body {
                    body.clone()
                } else {
                    Vec::from("Hello!")
                };
                resp.send(HttpStatus::OK, "text/plain", Some(&body[..]));
            }
        };

        resp
    });

    api.add_server("0.0.0.0:8081", default_api_handler.clone()).unwrap();
    api.add_server("0.0.0.0:8082", HttpServerHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Hello from 8082!"));
        resp
    })).unwrap();

    api.add_server("0.0.0.0:8083", default_api_handler.clone()).unwrap();
    api.add_server("0.0.0.0:8083", default_api_handler.clone()).unwrap();

    thread::sleep(Duration::from_secs(10));

    http.add_server("0.0.0.0:8080", HttpServerHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);

        let now = Local::now();

        let uri = if resp.r.uri.trim() == "/" {
            String::from("index.html")
        } else {
            let uri = String::from(resp.r.uri.trim_start_matches("/").trim_end_matches("/"));
            match std::fs::metadata(&uri) {
                Ok(m) => {
                    if m.is_dir() {
                        String::from(format!("{}/index.html", &uri))
                    } else {
                        uri
                    }
                },
                Err(_) => uri
            }    
        };

        println!("{}: [{:?}] {} -> {}", now.format("%Y-%m-%d %H:%M:%S"), thread::current().id(), resp.r.uri, uri);

        resp.send_file(&uri);
        resp
    })).unwrap();

    api.add_server("0.0.0.0:8084", default_api_handler.clone()).unwrap();
    api.remove_server_with_routes("0.0.0.0:8083").unwrap();

    api.remove_server_with_routes("0.0.0.0:8081").unwrap();
    api.add_server("0.0.0.0:8083", default_api_handler.clone()).unwrap();

    api.add_route("0.0.0.0:8083", "/a/b/c", None, RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Route: /a/b/c"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8083", "/a/b/*", None, RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Route: /a/b/*"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8084", "/x/y/*", None, RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Route: /x/y/*"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8084", "/x/y/*/", None, RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Route: /x/y/*/"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8084", "/x/y/*", Some(HttpMethod::PUT), RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Route: PUT + /x/y/*"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8084", "/x/y/*", Some(HttpMethod::POST), RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"Route: POST + /x/y/*"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8084", "~ ^/x+/y+/z+$", None, RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"RouteRE: ^/x+/y+/z+$"));
        resp
    })).unwrap();

    api.add_route("0.0.0.0:8084", "~ \\.(jpg|gif)$", None, RouteHandler::new(|r| -> HttpResponse {
        let mut resp = HttpResponse::new(r);
        resp.send(HttpStatus::OK, "text/plain", Some(b"RouteRE: \\.(jpg|gif)$"));
        resp
    })).unwrap();

    http.wait();
    api.wait();
}
