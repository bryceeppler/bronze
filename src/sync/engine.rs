use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use bronze_wire::{Precondition, Version};

use super::code::SyncCode;
use super::remote::{Pulled, Pushed, Remote};
use super::seal::{Seal, Unopenable};
use super::status::{Failure, Status};
use crate::board::Replica;

const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(60);

pub enum Event {
    Edited(Replica),
    Pulled(Result<Pulled, Failure>),
    Stop,
}

pub enum Delivery {
    Remote(Replica),
    Status(Status),
}

enum Server {
    Unknown,
    Missing,
    At(Version, Replica),
    Unopenable(Version, Unopenable),
}

enum Exchange {
    Pulled(Pulled),
    Pushed(Pushed),
}

pub struct Engine {
    remote: Remote,
    seal: Seal,
    local: Replica,
    server: Server,
    status: Status,
    retry_at: Option<Instant>,
    next_retry: Duration,
    watching: bool,
    events: Sender<Event>,
    deliver: Box<dyn Fn(Delivery) + Send>,
}

impl Engine {
    pub fn new(
        code: &SyncCode,
        local: Replica,
        events: Sender<Event>,
        deliver: impl Fn(Delivery) + Send + 'static,
    ) -> Engine {
        let derived = code.key.derive();
        Engine {
            remote: Remote::new(&code.server, &derived.vault, &derived.token),
            seal: Seal::new(&derived.vault, &derived.cipher),
            local,
            server: Server::Unknown,
            status: Status::Connecting,
            retry_at: None,
            next_retry: FIRST_RETRY,
            watching: false,
            events,
            deliver: Box::new(deliver),
        }
    }

    pub fn run(mut self, events: &Receiver<Event>) {
        loop {
            if self.retry_at.is_none() {
                self.reconcile();
            }
            self.watch();
            let first = match self.retry_at {
                Some(at) => match events.recv_timeout(at.saturating_duration_since(Instant::now())) {
                    Ok(event) => event,
                    Err(RecvTimeoutError::Timeout) => {
                        self.retry_at = None;
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                },
                None => match events.recv() {
                    Ok(event) => event,
                    Err(_) => return,
                },
            };
            for event in std::iter::once(first).chain(events.try_iter()) {
                match event {
                    Event::Stop => return,
                    Event::Edited(replica) => {
                        if grow(&mut self.local, &replica) {
                            self.retry_at = None;
                        }
                    }
                    Event::Pulled(pulled) => {
                        self.watching = false;
                        match pulled {
                            Ok(pulled) => self.pulled(pulled),
                            Err(failure) => self.failed(failure),
                        }
                    }
                }
            }
        }
    }

    fn reconcile(&mut self) {
        loop {
            let exchanged = match &self.server {
                Server::Unknown => self.remote.pull(None).map(Exchange::Pulled),
                Server::Missing => {
                    self.remote.push(Precondition::Create, &self.seal.seal(&self.local)).map(Exchange::Pushed)
                }
                Server::At(version, remote) if *remote != self.local => self
                    .remote
                    .push(Precondition::Replace(*version), &self.seal.seal(&self.local))
                    .map(Exchange::Pushed),
                Server::At(..) => return self.settle(Status::Synced),
                Server::Unopenable(_, why) => return self.settle(Status::Refused((*why).into())),
            };
            match exchanged {
                Ok(Exchange::Pulled(pulled)) => self.pulled(pulled),
                Ok(Exchange::Pushed(pushed)) => self.pushed(pushed),
                Err(failure) => return self.failed(failure),
            }
        }
    }

    fn pulled(&mut self, pulled: Pulled) {
        self.next_retry = FIRST_RETRY;
        match pulled {
            Pulled::Snapshot(version, _) if self.known() == Some(version) => {}
            Pulled::Snapshot(version, sealed) => {
                self.server = match self.seal.open(&sealed) {
                    Ok(remote) => {
                        if grow(&mut self.local, &remote) {
                            (self.deliver)(Delivery::Remote(remote.clone()));
                        }
                        Server::At(version, remote)
                    }
                    Err(why) => Server::Unopenable(version, why),
                }
            }
            Pulled::Unchanged => {}
            Pulled::Missing => self.server = Server::Missing,
        }
    }

    fn pushed(&mut self, pushed: Pushed) {
        self.next_retry = FIRST_RETRY;
        self.server = match pushed {
            Pushed::Written(version) => Server::At(version, self.local.clone()),
            Pushed::Stale => Server::Unknown,
            Pushed::Missing => Server::Missing,
        };
    }

    fn failed(&mut self, failure: Failure) {
        self.server = Server::Unknown;
        self.settle(failure.into());
        self.retry_at = Some(Instant::now() + self.next_retry);
        self.next_retry = (self.next_retry * 2).min(LAST_RETRY);
    }

    fn watch(&mut self) {
        let Some(seen) = self.known() else { return };
        if self.watching || self.retry_at.is_some() {
            return;
        }
        self.watching = true;
        let (remote, events) = (self.remote.clone(), self.events.clone());
        thread::Builder::new()
            .name("bronze-sync-watch".into())
            .spawn(move || _ = events.send(Event::Pulled(remote.pull(Some(seen)))))
            .expect("spawn the sync watcher");
    }

    fn known(&self) -> Option<Version> {
        match &self.server {
            Server::At(version, _) | Server::Unopenable(version, _) => Some(*version),
            Server::Unknown | Server::Missing => None,
        }
    }

    fn settle(&mut self, status: Status) {
        if status != self.status {
            eprintln!("bronze: sync {status}");
            self.status = status.clone();
            (self.deliver)(Delivery::Status(status));
        }
    }
}

fn grow(local: &mut Replica, other: &Replica) -> bool {
    let mut joined = local.clone();
    joined.join(other);
    let grew = joined != *local;
    *local = joined;
    grew
}
