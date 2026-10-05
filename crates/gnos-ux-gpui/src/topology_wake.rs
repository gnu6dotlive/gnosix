//! Suspend the scene topology consumer between events and its safety deadline.
use futures::{FutureExt, StreamExt, channel::mpsc::UnboundedReceiver};
use gnosix_hyprland::topology::TopologyWakeHint;
use std::{future::Future, time::Duration};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RuntimeWake {
    Event(TopologyWakeHint),
    Shutdown(&'static str),
    SafetyResync,
    Unavailable(&'static str),
}

pub(crate) async fn next_runtime_wake<F, T>(
    events: &mut UnboundedReceiver<TopologyWakeHint>,
    shutdown: &mut UnboundedReceiver<&'static str>,
    safety_after: Duration,
    timer: F,
) -> RuntimeWake
where
    F: FnOnce(Duration) -> T,
    T: Future<Output = ()>,
{
    let next_event = events.next().fuse();
    let next_signal = shutdown.next().fuse();
    let safety = timer(safety_after).fuse();
    futures::pin_mut!(next_event, next_signal, safety);
    futures::select_biased! {
        signal = next_signal => signal.map_or(RuntimeWake::Unavailable("shutdown_channel_closed"), RuntimeWake::Shutdown),
        event = next_event => event.map_or(RuntimeWake::Unavailable("event_channel_closed"), RuntimeWake::Event),
        () = safety => RuntimeWake::SafetyResync,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{channel::mpsc, future, task::noop_waker};
    use std::{
        cell::Cell,
        task::{Context, Poll},
    };

    #[test]
    fn idle_wait_arms_only_the_safety_deadline() {
        let (_sender, mut events) = mpsc::unbounded();
        let (_signals, mut shutdown) = mpsc::unbounded();
        let delay = Cell::new(Duration::ZERO);
        let mut waiting = Box::pin(next_runtime_wake(
            &mut events,
            &mut shutdown,
            Duration::from_secs(2),
            |duration| {
                delay.set(duration);
                future::pending()
            },
        ));
        assert_eq!(
            waiting
                .as_mut()
                .poll(&mut Context::from_waker(&noop_waker())),
            Poll::Pending
        );
        assert_eq!(
            delay.get(),
            Duration::from_secs(2),
            "idle must not schedule a 20ms polling tick"
        );
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use futures::{
        channel::{mpsc, oneshot},
        future,
        task::{ArcWake, waker},
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };

    struct WakeCount(AtomicUsize);
    impl ArcWake for WakeCount {
        fn wake_by_ref(this: &Arc<Self>) {
            this.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn event_wakes_suspended_consumer_without_poll_timer() {
        let (sender, mut events) = mpsc::unbounded();
        let (_signals, mut shutdown) = mpsc::unbounded();
        let count = Arc::new(WakeCount(AtomicUsize::new(0)));
        let waker = waker(Arc::clone(&count));
        let mut cx = Context::from_waker(&waker);
        let mut waiting = Box::pin(next_runtime_wake(
            &mut events,
            &mut shutdown,
            Duration::from_secs(2),
            |_| future::pending(),
        ));
        assert_eq!(waiting.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(count.0.load(Ordering::Relaxed), 0);
        sender
            .unbounded_send(TopologyWakeHint::SemanticChanged("workspace".into()))
            .unwrap();
        assert!(count.0.load(Ordering::Relaxed) > 0);
        assert_eq!(
            waiting.as_mut().poll(&mut cx),
            Poll::Ready(RuntimeWake::Event(TopologyWakeHint::SemanticChanged(
                "workspace".into()
            )))
        );
    }

    #[test]
    fn signal_has_priority_without_consuming_queued_event() {
        let (sender, mut events) = mpsc::unbounded();
        let (signals, mut shutdown) = mpsc::unbounded();
        sender.unbounded_send(TopologyWakeHint::Startup).unwrap();
        signals.unbounded_send("SIGTERM").unwrap();
        let wake = futures::executor::block_on(next_runtime_wake(
            &mut events,
            &mut shutdown,
            Duration::from_secs(2),
            |_| future::ready(()),
        ));
        assert_eq!(wake, RuntimeWake::Shutdown("SIGTERM"));
        assert_eq!(events.try_recv().unwrap(), TopologyWakeHint::Startup);
    }

    #[test]
    fn safety_deadline_survives_quiet_or_degraded_event_stream() {
        let (_sender, mut events) = mpsc::unbounded();
        let (_signals, mut shutdown) = mpsc::unbounded();
        let (clock, elapsed) = oneshot::channel();
        let count = Arc::new(WakeCount(AtomicUsize::new(0)));
        let waker = waker(Arc::clone(&count));
        let mut cx = Context::from_waker(&waker);
        let mut waiting = Box::pin(next_runtime_wake(
            &mut events,
            &mut shutdown,
            Duration::from_secs(2),
            |_| async { elapsed.await.unwrap() },
        ));
        assert_eq!(waiting.as_mut().poll(&mut cx), Poll::Pending);
        clock.send(()).unwrap();
        assert_eq!(
            waiting.as_mut().poll(&mut cx),
            Poll::Ready(RuntimeWake::SafetyResync)
        );
    }

    #[test]
    fn events_coalesce_without_loss_and_timer_can_be_cancelled() {
        let (sender, mut events) = mpsc::unbounded();
        let (_signals, mut shutdown) = mpsc::unbounded();
        sender
            .unbounded_send(TopologyWakeHint::StreamDisconnected)
            .unwrap();
        sender
            .unbounded_send(TopologyWakeHint::StreamConnected)
            .unwrap();
        sender
            .unbounded_send(TopologyWakeHint::SafetyResync)
            .unwrap();
        let (clock, elapsed) = oneshot::channel::<()>();
        let wake = futures::executor::block_on(next_runtime_wake(
            &mut events,
            &mut shutdown,
            Duration::from_secs(2),
            |_| async {
                let _ = elapsed.await;
            },
        ));
        assert_eq!(
            wake,
            RuntimeWake::Event(TopologyWakeHint::StreamDisconnected)
        );
        assert!(clock.is_canceled());
        assert_eq!(
            events.try_recv().unwrap(),
            TopologyWakeHint::StreamConnected
        );
        assert_eq!(events.try_recv().unwrap(), TopologyWakeHint::SafetyResync);
    }

    #[test]
    fn closed_channel_fails_explicitly_instead_of_busy_looping() {
        let (sender, mut events) = mpsc::unbounded();
        let (_signals, mut shutdown) = mpsc::unbounded();
        drop(sender);
        let wake = futures::executor::block_on(next_runtime_wake(
            &mut events,
            &mut shutdown,
            Duration::from_secs(2),
            |_| future::pending(),
        ));
        assert_eq!(wake, RuntimeWake::Unavailable("event_channel_closed"));
    }
}
