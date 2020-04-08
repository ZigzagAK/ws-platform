/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::io::prelude::*;
use mio::net::TcpStream;
use std::ops::Deref;

pub struct Buffer {
    data: Vec<u8>,
    rpos: usize,
    wpos: usize,
    end: usize
}

impl Default for Buffer {
    fn default() -> Buffer {
        Buffer {
            data: Vec::with_capacity(4096),
            rpos: 0,
            wpos: 0,
            end: 0
        }
    }
}

impl Deref for Buffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.data[..self.end]
    }
}

impl Buffer {
    pub fn reset(&mut self) {
        self.rpos = 0;
        self.wpos = 0;
        self.end = 0;
        self.data.clear();
    }

    pub fn getc(&mut self) -> u8 {
        let c = self.data[self.rpos];
        self.rpos += 1;
        c
    }

    pub fn read(&mut self, stream: &mut TcpStream) -> std::io::Result<(bool, usize)> {
        if self.end >= self.data.len() / 2 {
            self.data.resize(match self.data.len() {
                0 => 4096,
                len => len * 2
            }, 0);
        }
        let sz = stream.read(&mut self.data)?;
        self.end += sz;
        Ok((sz == 0, sz))
    }

    pub fn write(&mut self, stream: &mut TcpStream) -> std::io::Result<(bool, usize)> {
        if self.end > self.wpos {
            let sz = stream.write(&mut self.data[self.wpos..self.end])?;
            self.wpos += sz;
            return Ok((self.wpos == self.end, sz));
        }
        Ok((true, 0))
    }

    pub fn extend(&mut self, slice: &[u8]) {
        self.data.extend_from_slice(slice);
        self.end += slice.len();
    }

    pub fn end(&self) -> bool {
        self.rpos >= self.end
    }

    pub fn tail(&mut self) -> &[u8] {
        let data = &self.data[self.rpos..self.end];
        self.rpos = self.end;
        data
    }

    pub fn chunk(&mut self, len: usize) -> &[u8] {
        let end = std::cmp::min(self.rpos + len, self.end);
        let data = &self.data[self.rpos..end];
        self.rpos = end;
        data
    }

    pub fn len(&self) -> usize {
        self.end - self.rpos
    }

    pub fn wpos(&self) -> usize {
        self.wpos
    }

    pub fn rpos(&self) -> usize {
        self.rpos
    }
}
