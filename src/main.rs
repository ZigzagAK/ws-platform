/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

extern crate web_server;
extern crate yaml_rust;

use web_server::core::*;
use web_server::http::*;
use web_server::tcp::tcp::*;

fn main() {

    let conf_main = "
---
error_log: error.log
";

    let conf_http = "
---
http:
  error_log: error.log
  log_formats:
    - log_format:
        name: default
        format: '${request_start} ${local_time} [${remote_addr}] ${protocol} ${request_uri} ${request_time}ms'
    - log_format:
        name: upstream
        format: '${request_start} ${local_time} [${remote_addr}] ${protocol} ${request_uri} ${request_time}ms ${upstream_name} ${upstream_addr} ${upstream_status} ${upstream_response_time}ms'
  workgroups:
    - workgroup:
        name: default
        thread_pool_size: 10
        socket_pool_size: 4096
    - workgroup:
        name: proxy
        event_pool_size: 12
        thread_pool_size: 0
        socket_pool_size: 512
    - workgroup:
        name: app
        event_pool_size: 12
        thread_pool_size: 0
        socket_pool_size: 512
    - workgroup:
        name: group1
        event_pool_size: 12
        thread_pool_size: 12
        socket_pool_size: 1024
    - workgroup:
        name: group2
        thread_pool_size: 10
        socket_pool_size: 1024
  upstreams:
    - upstream:
        name: u1
        max_active: 100
        keepalive: 100
        servers:
          - server:
              address: 127.0.0.1:8081
              max_active: 100
              keepalive: 100
          - server:
              address: 127.0.0.2:8081
              max_active: 100
              keepalive: 100
          - server:
              address: 127.0.0.3:8081
              max_active: 100
              keepalive: 100
              backup: true
    - upstream:
        name: nginx
        least_conn: true
        max_active: 500
        keepalive: 500
        keepalive_timeout: 60000
        keepalive_requests: 10000
        servers:
          - server:
              address: 127.0.0.1:6000
              max_active: 100
              keepalive: 100
          - server:
              address: 127.0.0.2:6000
              max_active: 100
              keepalive: 100
          - server:
              address: 127.0.0.3:6000
              max_active: 100
              keepalive: 100
              backup: true
    - upstream:
        name: nginx_backup
        least_conn: true
        max_active: 500
        keepalive: 500
        keepalive_timeout: 60000
        keepalive_requests: 10000
        servers:
          - server:
              address: 127.1.0.1:6000
              max_active: 100
              keepalive: 100
          - server:
              address: 127.1.0.2:6000
              max_active: 100
              keepalive: 100
          - server:
              address: 127.1.0.3:6000
              max_active: 100
              keepalive: 100
              backup: true
  servers:
    - server:
        bind: 0.0.0.0:9091
        request_timeout: 10000
        response_timeout: 10000
        keepalive_timeout: 60000
        keepalive_requests: 10000
        group: proxy
        access_log:
          filename: 9091.log
          format: upstream
        add_headers:
          AddHeaderServer1: test2
          AddHeaderServer2: test2
          Multi:
            - val1
            - val2
        add_args:
          arg1: v1
          arg2: v2
          arg_multi:
            - 1
            - 2
        clear_args:
          - deleted1
        set_request_headers:
          x-proxy-server: Hello1
        clear_request_headers:
          - xxx
        clear_headers:
          - Date
        routes:
          - route:
              match: /*
              clear_args:
                - deleted2
              add_args:
                arg3: v3
              set_request_headers:
                x-proxy-route: Hello2
              add_headers:
                AddHeader: test
              proxy:
                pass: nginx
                backup: nginx_backup
                proxy_timeout: 5000
    - server:
        bind: 0.0.0.0:9092
        request_timeout: 10000
        response_timeout: 10000
        keepalive_timeout: 60000
        keepalive_requests: 10000
        group: proxy
        routes:
          - route:
              match: /*
              proxy:
                pass: nginx
                backup: nginx_backup
                proxy_timeout: 5000
    - server:
        bind: 0.0.0.0:8000
        group: group1
        routes:
          - route:
              match: /hello
              echo: Hello from 8000/*
    - server:
        bind: 0.0.0.0:8000
        group: group1
        virtual_host: server1
        routes:
          - route:
              match: /hello
              echo: Hello from 8000/server1
    - server:
        bind: 0.0.0.0:8000
        group: group1
        virtual_host: server2
        routes:
          - route:
              match: /hello
              echo: Hello from 8000/server2
    - server:
        bind: 0.0.0.0:8080
        group: app
        error_log: error8080.log
        request_timeout: 10000
        response_timeout: 10000
        keepalive_timeout: 60000
        keepalive_requests: 10000
        access_log:
          filename: 8080.log
          buffer_size: 16384
          format: default
        add_headers:
          AddHeaderServer: test      
        routes:
          - route:
              match: /upstream/status
              upstream_status: get upstream status
          - route:
              match: '@internal'
              echo: Hello from internal!
          - route:
              match: /to_internal
              rewrite: '@internal'
          - route:
              match: ~ ^/to_internal/re
              rewrite: '@internal'
          - route:
              match: '@unauthorized'
              echo:
                text: Unauthorized
                status: 401
          - route:
              match: /unauthorized
              basic: '@unauthorized'
          - route:
              match: /unauthorized2
              basic: /unauthorized2
          - route:
              match: ~ ^/re/
              method: GET
              echo: re:GET
          - route:
              match: ~ ^/re/
              method: POST
              echo: re:POST
          - route:
              match: /ping
              rewrite: xxx
              method: PUT
              echo: echo:PUT
          - route:
              match: /ping
              method: GET
              echo: echo:GET
          - route:
              match: /ping
              method: POST
              echo: echo:POST
          - route:
              match: /lua
              lua: |
                return 'Hello from LUA!'
          - route:
              match: /api/*
              index: site
          - route:
              match: /demo/*
              index: site
          - route:
              match: /api/customers/{customer_id}/*
              echo: CUSTOMER_ID=${customer_id},hello,${arg_a},${http_Host}
          - route:
              match: /api/subscribers/{subscriber_id}/*
              echo: SUBSCRIBER_ID=${subscriber_id},hello,${arg_a},${http_Host}
          - route:
              match: '~ ^/api/re/customers/(?P<customer_id>[0-9]+)/.*'
              echo: CUSTOMER_ID=${customer_id},hello,${arg_a},${http_Host}
    - server:
        bind: 0.0.0.0:8070
        group: app
        access_log:
          filename: 8070.log
          buffer_size: 1048576
          format: default
        routes:
          - route:
              match: /ping
              echo: echo
          - route:
              match: /vartest
              vars:
                v1: 'host=${http_Host}'
                v2: xxx
              add_headers:
                X: ${v1}
                Y: ${v2}
              echo: ${v1},${v2}
    - server:
        bind: 0.0.0.0:8081
        group: group2
        routes:
          - route:
              match: /ping
              echo: echo
          - route:
              match: /python
              python: |
                import datetime, sys as s, os as o
                import math as m,time
                import numbers
                response.text = 'Hello from Python! Now is: {}'.format(datetime.datetime.now())
          - route:
              match: /python2
              python: |
                response.text = 'Hello from Python!'
          - route:
              match: /api/*
              index: site
          - route:
              match: /demo/*
              index: site
    - server:
        bind: 0.0.0.0:9090
        routes:
          - route:
              match: /*
              proxy:
                pass: 127.0.0.1:6000
                keepalive: 500
                max_active: 500
    - server:
        bind: 0.0.0.0:9093
        group: proxy
        routes:
          - route:
              match: /*
              proxy: u1
";

    CoreModule::configure();
    CoreModule::config_parse(conf_main).unwrap();

    HttpModule::configure();
    HttpModule::config_parse(conf_http).unwrap();

    TcpModule::configure();

    CoreModule::activate();
    HttpModule::activate();
    TcpModule::activate();

    // HttpModule::deactivate();
    // TcpModule::deactivate();

    HttpModule::wait();
    TcpModule::wait();
    CoreModule::wait();
}