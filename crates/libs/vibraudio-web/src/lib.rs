#[cfg(target_arch = "wasm32")]
mod backend;

#[cfg(target_arch = "wasm32")]
pub use backend::FRAMES;
#[cfg(target_arch = "wasm32")]
pub use backend::WebBackend;

#[cfg(target_arch = "wasm32")]
mod yield_impl {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::{Context, Poll, Waker};
    use wasm_bindgen::prelude::*;

    thread_local! {
        static YIELDER: RefCell<Option<Yielder>> = const { RefCell::new(None) };
    }

    struct Yielder {
        _port1: web_sys::MessagePort,
        port2: web_sys::MessagePort,
        wakers: Rc<RefCell<VecDeque<Waker>>>,
    }

    fn ensure_yielder() {
        YIELDER.with(|slot| {
            if slot.borrow().is_some() {
                return;
            }

            let channel = web_sys::MessageChannel::new().expect("MessageChannel");
            let port1 = channel.port1();
            let port2 = channel.port2();

            let wakers: Rc<RefCell<VecDeque<Waker>>> = Rc::new(RefCell::new(VecDeque::new()));

            let wakers_clone = wakers.clone();
            let cb = Closure::<dyn FnMut()>::new(move || {
                // Drain all waiters. Waker::wake() does not allocate JS objects.
                let drained: Vec<Waker> = {
                    let mut q = wakers_clone.borrow_mut();
                    q.drain(..).collect()
                };
                for w in drained {
                    w.wake();
                }
            });
            port1.set_onmessage(Some(cb.as_ref().unchecked_ref()));
            cb.forget(); // one time for the whole app — not a leak

            *slot.borrow_mut() = Some(Yielder {
                _port1: port1,
                port2,
                wakers,
            });
        });
    }

    struct YieldFast {
        polled: bool,
    }

    impl Future for YieldFast {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.polled {
                return Poll::Ready(());
            }
            self.polled = true;

            ensure_yielder();

            YIELDER.with(|slot| {
                let borrow = slot.borrow();
                let y = borrow.as_ref().expect("yielder");
                y.wakers.borrow_mut().push_back(cx.waker().clone());
                y.port2.post_message(&JsValue::NULL).expect("post_message");
            });

            Poll::Pending
        }
    }

    pub async fn yield_fast() {
        YieldFast { polled: false }.await;
    }
}

#[cfg(target_arch = "wasm32")]
pub use yield_impl::yield_fast;
