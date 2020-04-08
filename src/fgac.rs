/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::ops::{ Deref, DerefMut };
use std::sync::{ MutexGuard, Arc, Mutex, Condvar };

pub struct FGACData<T> {
    mutex: Mutex<T>,
    ev: Condvar
}

#[derive(Clone)]
pub struct FGAC<T> {
    inner: Arc<FGACData<T>>
}

pub struct FGACScopedLock<'a, T> {
    fgac: &'a FGAC<T>,
    guard: Option<MutexGuard<'a, T>>
}

impl<'a, T> Deref for FGACScopedLock<'a, T> {
    type Target = T;
    fn deref(&self) -> &'_ Self::Target {
        self.guard.as_ref().unwrap()
    }
}

impl<'a, T> DerefMut for FGACScopedLock<'a, T> {
    fn deref_mut(&mut self) -> &'_ mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}

impl<'a, T> FGACScopedLock<'a, T> {
    pub fn new(fgac: &'a FGAC<T>) -> FGACScopedLock<'a, T>
    {
        FGACScopedLock {
            fgac: fgac,
            guard: Some(fgac.mutex.lock().unwrap())
        }
    }

    pub fn map<F>(&mut self, mut f: F) -> &mut FGACScopedLock<'a, T>
    where
        F: FnMut(&FGAC<T>, &mut T)
    {
        f(&self.fgac, self.guard.as_mut().unwrap());
        self
    }

    pub fn wait(&mut self) -> &mut FGACScopedLock<'a, T> {
        self.guard = self.guard.take().map(|guard| {
            self.fgac.ev.wait(guard).unwrap()
        });
        self
    }
}

impl<T> FGACData<T> {
    pub fn wait(&self) {
        let _ = self.ev.wait(self.mutex.lock().unwrap()).unwrap();
    }

    pub fn notify_one(&self) {
        self.ev.notify_one()
    }

    pub fn notify_all(&self) {
        self.ev.notify_all()
    }
}

impl<T> FGAC<T> {
    pub fn new(obj: T) -> FGAC<T> {
        FGAC {
            inner: Arc::new(FGACData {
                mutex: Mutex::new(obj),
                ev: Condvar::new()
            })
        }
    }
}

impl <T> Deref for FGAC<T> {
    type Target = FGACData<T>;
    fn deref(&self) -> &FGACData<T> {
        &self.inner
    }
}