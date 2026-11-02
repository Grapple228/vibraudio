mod backend;

pub use backend::FRAMES;
pub use backend::WebBackend;

#[cfg(target_arch = "wasm32")]
mod yield_impl {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;

    thread_local! {
        static YIELD: RefCell<Option<Yielder>> = const { RefCell::new(None) };
    }

    struct Yielder {
        _port1: web_sys::MessagePort,
        port2: web_sys::MessagePort,
        queue: Rc<RefCell<VecDeque<js_sys::Function>>>,
    }

    fn ensure_yielder() {
        YIELD.with(|slot| {
            if slot.borrow().is_some() {
                return;
            }

            let channel = web_sys::MessageChannel::new().expect("MessageChannel");
            let port1 = channel.port1();
            let port2 = channel.port2();

            let queue: Rc<RefCell<VecDeque<js_sys::Function>>> =
                Rc::new(RefCell::new(VecDeque::new()));

            let queue_clone = queue.clone();
            let cb = Closure::<dyn FnMut()>::new(move || {
                let mut q = queue_clone.borrow_mut();
                while let Some(resolve) = q.pop_front() {
                    let _ = resolve.call0(&JsValue::NULL);
                }
            });
            port1.set_onmessage(Some(cb.as_ref().unchecked_ref()));
            cb.forget(); // один раз за всё приложение — не утечка

            *slot.borrow_mut() = Some(Yielder {
                _port1: port1,
                port2,
                queue,
            });
        });
    }

    /// Макротаск-yield через MessageChannel. ~0.1–1 мс.
    /// Гарантированно отдаёт управление event loop (UI, другие таски).
    pub async fn yield_fast() {
        ensure_yielder();

        let promise = js_sys::Promise::new(&mut |resolve, _| {
            YIELD.with(|slot| {
                let borrow = slot.borrow();
                let y = borrow.as_ref().expect("yielder");
                y.queue.borrow_mut().push_back(resolve);
                y.port2.post_message(&JsValue::NULL).expect("post_message");
            });
        });

        let _ = promise.await;
    }
}

#[cfg(target_arch = "wasm32")]
pub use yield_impl::yield_fast;
