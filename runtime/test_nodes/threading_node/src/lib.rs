use std::thread;
use std::sync::mpsc;

pub struct ThreadingNode {
    pub num_threads: usize,
}

impl ThreadingNode {
    pub fn new(num_threads: usize) -> Self {
        Self { num_threads }
    }

    pub fn process(&self, data: Vec<i32>) -> Vec<i32> {
        let (tx, rx) = mpsc::channel();
        
        for value in data {
            let tx = tx.clone();
            thread::spawn(move || {
                let result = value * 2;
                tx.send(result).unwrap();
            });
        }
        
        drop(tx);
        rx.iter().collect()
    }
}
