use std::{
    error::Error,
    result,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
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

#[derive(Eq, PartialEq, Debug)]
struct Foo {
    value: usize,
}

type Processor<T> = fn(receiver: Receiver<T>, sender: Sender<T>);

pub fn seq_parallel<T>(ts: Vec<T>, fs: Vec<Processor<T>>) -> Result<Vec<T>>
where
    T: Send + 'static,
{
    let (tx, mut rx) = channel();

    thread::spawn(move || {
        for t in ts {
            tx.send(t).unwrap();
        }
    });

    for f in fs {
        let (tx, rx_next) = channel();
        thread::spawn(move || {
            f(rx, tx);
        });
        rx = rx_next
    }

    let mut res = Vec::new();
    for r in rx {
        res.push(r)
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc::Receiver, sync::mpsc::Sender};

    use crate::process::cpr::{seq_parallel, seq_serial, Foo, Processor};

    #[test]
    fn test_seq_serial() {
        let f = |mut f: Foo| {
            f.value = f.value + 1;
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
        let f: Processor<Foo> = |rx: Receiver<Foo>, tx: Sender<Foo>| {
            for mut f in rx {
                f.value = f.value + 1;
                tx.send(f).unwrap();
            }
        };

        assert_eq!(
            vec![Foo { value: 4 }, Foo { value: 14 }, Foo { value: 24 }],
            seq_parallel::<Foo>(
                vec![Foo { value: 1 }, Foo { value: 11 }, Foo { value: 21 }],
                vec![f, f, f]
            )
            .unwrap()
        )
    }
}
