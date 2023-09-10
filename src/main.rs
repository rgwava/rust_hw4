//直接cargo run即可
use std::task::Context;
use std::task::Waker;
use std::future::Future;
use std::task::RawWaker;
use std::task::RawWakerVTable;
use std::task::Poll;
use std::pin::Pin;
use std::task::Wake;
use std::sync::Arc;
use std::time::Duration;
use std::sync::Mutex;
use std::sync::Condvar;
use std::collections::VecDeque;
use std::cell::RefCell;
use async_std::task::spawn;
use futures::future::BoxFuture;
scoped_tls::scoped_thread_local!(static SIGNAL:Arc<Signal>);
scoped_tls::scoped_thread_local!(static RUNNABLE:Mutex<VecDeque<Arc<Task>>>);
struct Demo;
impl Future for Demo{
    type Output=();
    fn poll(self:std::pin::Pin<&mut Self>,_cx:&mut std::task::Context<'_>)
    ->std::task::Poll<Self::Output>{
        println!("hello");
        std::task::Poll::Ready(())
    }
}

enum State{
    Empty,
    Waiting,
    Notified,
}

struct Signal{
    state:Mutex<State>,
    cond:Condvar,
}
impl Signal{
    fn wait(&self){
        let mut state=self.state.lock().unwrap();
        match *state{
            State::Notified=>*state=State::Empty,
            State::Waiting=>{
                panic!("multiple wait");
            }
            State::Empty=>{
                *state=State::Waiting;
                while let State::Waiting=*state{
                    state=self.cond.wait(state).unwrap();
                }
            }
        }
    }
    fn notify(&self){
        let mut state=self.state.lock().unwrap();
        match *state{
            State::Notified=>{}
            State::Empty=>*state=State::Notified,
            State::Waiting=>{
               *state=State::Empty;
               self.cond.notify_one();
            }
            
        }
    }
    fn new()->Signal{
        Signal{
            state:Mutex::new(State::Notified),
            cond:Condvar::new(),
        }
    }
}
impl Wake for Signal{
    fn wake(self:Arc<Self>){
        self.notify();
    }
}
// #[stable(feature="wake_trait",since="1.51.0")]
// impl<W:Wake+Send+Sync+'static>From<Arc<W>> for Waker??

//--------------------------------
// fn dummy_waker()->Waker{
//     static DATA:()=();
//     unsafe{
//         Waker::from_raw(RawWaker::new(&DATA,&VTABLE))
//     }
// }
// const VTABLE:RawWakerVTable=
// RawWakerVTable::new(vtable_clone,vtable_wake,vtable_wake_by_ref,vtable_drop);
// unsafe fn vtable_clone(_p:*const ())->RawWaker{
//     RawWaker::new(_p,&VTABLE)
// }
// unsafe fn vtable_wake(_p:*const ()){}
// unsafe fn vtable_wake_by_ref(_p:*const ()){}
// unsafe fn vtable_drop(_p:*const ()){}

//--------------------------------------
fn block_on<F:Future>(future:F)->F::Output{
    let mut fut:Pin<&mut F>=std::pin::pin!(future);
    let signal:Arc<Signal>=Arc::new(Signal::new());
    let waker:Waker=Waker::from(signal.clone());

    let mut cx:Context<'_>=Context::from_waker(&waker);
    let runnable=Mutex::new(VecDeque::with_capacity(1024));
    SIGNAL.set(&signal,||{
        RUNNABLE.set(&runnable,||{
            loop{
                if let Poll::Ready(output)=fut.as_mut().poll(&mut cx){
                    return output;
                }
                while let Some(task) = runnable.lock().unwrap().pop_front() {
                    let waker = Waker::from(task.clone());
                    let mut cx = Context::from_waker(&waker);
                    let _ = task.future.borrow_mut().as_mut().poll(&mut cx);
                }
                signal.wait();  
            }
        })
    })
}
struct Task{
    future:RefCell<BoxFuture<'static,()>>,
    signal:Arc<Signal>,
}
unsafe impl Send for Task{}
unsafe impl Sync for Task{}
impl Wake for Task{
    fn wake(self:Arc<Self>){
        RUNNABLE.with(|runnable|runnable.lock().unwrap().push_back(self.clone()));
        self.signal.notify();
    }
}
async fn demo(){
    let(tx,rx)=async_channel::bounded(1);
    spawn(demo2(tx));
    println!("hello world!");
    let _=rx.recv().await;
}

async fn demo2(tx:async_channel::Sender<()>){
    println!("hello world2!");
    let _ =tx.send(()).await;
}
fn main(){
    block_on(demo());
}
