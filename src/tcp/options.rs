/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::time::Duration;

#[derive(Clone, Copy)]
pub struct TcpOptions {
    pub read_timeout: Option<Duration>,
    pub send_timeout: Option<Duration>
}