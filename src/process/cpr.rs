use std::{
    error::Error,
    result,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread::{self, JoinHandle},
};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub fn seq_serial<T, F>(ts: Vec<T>, fs: Vec<F>) -> Result<Vec<T>>
where
    F: Fn(T) -> Result<T>,
{
    let mut res = Vec::new();
    for mut t in ts {
        for f in &fs {
            t = f(t)?;
        }
        res.push(t)
    }
    Ok(res)
}

type Processor<T> = fn(receiver: Receiver<T>, sender: SyncSender<T>);

pub fn seq_parallel<T>(ts: Vec<T>, fs: Vec<Processor<T>>) -> Result<Vec<T>>
where
    T: Send + 'static,
{
    let (tx, mut rx) = sync_channel(0);
    let mut threads = Vec::new();

    threads.push(thread::spawn(move || {
        for t in ts {
            if let Err(e) = tx.send(t) {
                panic!("{}", e);
            }
        }
    }));

    for f in fs {
        let (tx, rx_next) = sync_channel(0);
        threads.push(thread::spawn(move || {
            f(rx, tx);
        }));
        rx = rx_next
    }

    let mut res = Vec::new();
    for r in rx {
        res.push(r)
    }
    threads.into_iter().try_for_each(JoinHandle::join).unwrap();
    Ok(res)
}

type Processor2<T, E> = fn(arg: T) -> result::Result<T, E>;

pub fn seq_parallel_abstract<T, E>(
    ts: Vec<T>,
    fs: Vec<Processor2<T, E>>,
) -> result::Result<Vec<T>, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    let (tx, mut rx) = sync_channel(0);

    // producer
    thread::spawn(move || {
        for t in ts {
            if tx.send(Ok(t)).is_err() {
                return;
            }
        }
    });

    for f in fs {
        let (tx, rx_next) = sync_channel(0);
        thread::spawn(move || {
            for res in rx {
                match res {
                    Ok(t) => match f(t) {
                        Ok(t) => {
                            if tx.send(Ok(t)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }
        });
        rx = rx_next
    }

    rx.iter().collect::<result::Result<Vec<T>, E>>()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, SyncSender};

    use crate::process::cpr::{seq_parallel, seq_parallel_abstract, seq_serial, Processor};

    #[derive(Eq, PartialEq, Debug)]
    struct Foo {
        value: usize,
    }

    #[test]
    fn test_seq_serial() {
        let f = |mut f: Foo| {
            f.value += 1;
            Ok(f)
        };

        assert_eq!(
            vec![Foo { value: 4 }, Foo { value: 14 }, Foo { value: 24 }],
            seq_serial(
                vec![Foo { value: 1 }, Foo { value: 11 }, Foo { value: 21 }],
                vec![f, f, f]
            )
            .unwrap()
        )
    }

    #[test]
    fn test_seq_parallel() {
        let f: Processor<Foo> = |rx: Receiver<Foo>, tx: SyncSender<Foo>| {
            for mut f in rx {
                f.value += 1;
                tx.send(f).unwrap();
            }
        };

        assert_eq!(
            vec![Foo { value: 4 }, Foo { value: 14 }, Foo { value: 24 },],
            seq_parallel::<Foo>(
                vec![Foo { value: 1 }, Foo { value: 11 }, Foo { value: 21 }],
                vec![f, f, f]
            )
            .unwrap()
        )
    }

    #[test]
    fn test_seq_parallel_abstract() {
        let f = |mut f: Foo| {
            f.value += 1;
            Ok(f)
        };

        assert_eq!(
            vec![Foo { value: 4 }, Foo { value: 14 }, Foo { value: 24 }],
            seq_parallel_abstract::<Foo, String>(
                vec![Foo { value: 1 }, Foo { value: 11 }, Foo { value: 21 }],
                vec![f, f, f]
            )
            .unwrap()
        )
    }
}
