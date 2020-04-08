/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::rc::Rc;

pub struct Handler<Args, Output> {
    fun: Rc<Box<dyn Fn(Args) -> Output>>
}

impl<Args, Output> Clone for Handler<Args, Output> {
    fn clone(&self) -> Self {
        Handler {
            fun: Rc::clone(&self.fun)
        }
    }
}

impl<Args, Output> Handler<Args, Output> {
    pub fn new<F: 'static>(fun: F) -> Handler<Args, Output>
    where F: Fn(Args) -> Output {
        Handler { fun: Rc::new(Box::new(fun)) }
    }

    pub fn handle(&self, args: Args) -> Output {
        (self.fun)(args)
    }
}

pub mod sync {
    use std::sync::Mutex;
    use std::sync::Arc;

    pub struct Handler<Args, Output> {
        fun: Arc<Box<dyn Fn(Args) -> Output + 'static + Sync + Send>>
    }

    impl<Args, Output> Clone for Handler<Args, Output> {
        fn clone(&self) -> Self {
            Handler {
                fun: Arc::clone(&self.fun)
            }
        }
    }

    impl<Args, Output> Handler<Args, Output> {
        pub fn new<F: 'static + Sync + Send>(fun: F) -> Handler<Args, Output>
        where F: Fn(Args) -> Output {
            Handler { fun: Arc::new(Box::new(fun)) }
        }

        pub fn handle(&self, args: Args) -> Output {
            (self.fun)(args)
        }
    }
    
    pub struct MutHandler<Args, Output> {
        fun: Arc<Mutex<dyn FnMut(Args) -> Output + 'static + Sync + Send>>
    }

    impl<Args, Output> Clone for MutHandler<Args, Output> {
        fn clone(&self) -> Self {
            MutHandler {
                fun: Arc::clone(&self.fun)
            }
        }
    }

    impl<Args, Output> MutHandler<Args, Output> {
        pub fn new<F: 'static + Sync + Send>(fun: F) -> MutHandler<Args, Output>
        where F: FnMut(Args) -> Output {
            MutHandler { fun: Arc::new(Mutex::new(fun)) }
        }

        pub fn handle(&mut self, args: Args) -> Output {
            (&mut *self.fun.lock().unwrap())(args)
        }
    }

    // pub type Handler<Args, Output> = MutHandler<Args, Output>;

    pub struct RefHandler<Args, Output> {
        fun: Arc<Box<dyn Fn(&mut Args) -> Output + 'static + Sync + Send>>
    }

    impl<Args, Output> Clone for RefHandler<Args, Output> {
        fn clone(&self) -> Self {
            RefHandler {
                fun: Arc::clone(&self.fun)
            }
        }
    }

    impl<Args, Output> RefHandler<Args, Output> {
        pub fn new<F: 'static + Sync + Send>(fun: F) -> RefHandler<Args, Output>
        where F: Fn(&mut Args) -> Output {
            RefHandler { fun: Arc::new(Box::new(fun)) }
        }

        pub fn handle(&self, args: &mut Args) -> Output {
            (self.fun)(args)
        }
    }

    pub struct ConstRefHandler<Args, Output> {
        fun: Arc<Box<dyn Fn(&Args) -> Output + 'static + Sync + Send>>
    }

    impl<Args, Output> Clone for ConstRefHandler<Args, Output> {
        fn clone(&self) -> Self {
            ConstRefHandler {
                fun: Arc::clone(&self.fun)
            }
        }
    }

    impl<Args, Output> ConstRefHandler<Args, Output> {
        pub fn new<F: 'static + Sync + Send>(fun: F) -> ConstRefHandler<Args, Output>
        where F: Fn(&Args) -> Output {
            ConstRefHandler { fun: Arc::new(Box::new(fun)) }
        }

        pub fn handle(&self, args: &Args) -> Output {
            (self.fun)(args)
        }
    }

    pub struct RefMutHandler<Args, Output> {
        fun: Arc<Mutex<dyn FnMut(&mut Args) -> Output + 'static + Sync + Send>>
    }

    impl<Args, Output> Clone for RefMutHandler<Args, Output> {
        fn clone(&self) -> Self {
            RefMutHandler {
                fun: Arc::clone(&self.fun)
            }
        }
    }

    impl<Args, Output> RefMutHandler<Args, Output> {
        pub fn new<F: 'static + Sync + Send>(fun: F) -> RefMutHandler<Args, Output>
        where F: FnMut(&mut Args) -> Output {
            RefMutHandler { fun: Arc::new(Mutex::new(fun)) }
        }

        pub fn handle(&mut self, args: &mut Args) -> Output {
            let f = &mut *self.fun.lock().unwrap();
            (f)(args)
        }
    }
}