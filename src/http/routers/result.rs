use std::ops::{ Deref, DerefMut };
use std::sync::{ RwLockWriteGuard, RwLockReadGuard };

pub struct RouteResultMut<'a, Context> {
    _write: RwLockWriteGuard<'a, ()>,
    pub context: &'a mut Context
}

impl<'a, Context> Deref for RouteResultMut<'a, Context> {
    type Target = Context;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl<'a, Context> DerefMut for RouteResultMut<'a, Context> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

impl<'a, Context> RouteResultMut<'a, Context> {
    pub fn new(guard: RwLockWriteGuard<'a, ()>, context: &'a mut Context) -> RouteResultMut<'a, Context> {
        RouteResultMut {
            _write: guard,
            context: context
        }
    }
}

pub struct RouteResult<'a, Context> {
    _read: RwLockReadGuard<'a, ()>,
    pub context: &'a Context
}

impl<'a, Context> Deref for RouteResult<'a, Context> {
    type Target = Context;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl<'a, Context> RouteResult<'a, Context> {
    pub fn new(guard: RwLockReadGuard<'a, ()>, context: &'a Context) -> RouteResult<'a, Context> {
        RouteResult {
            _read: guard,
            context: context
        }
    }
}
