use std::collections::HashMap;
use std::task::Poll;
// Time wheel algorithem impl

struct Slot<T> {
    round: u64,
    t: T,
}

pub struct TimeWheel<T: Clone + Default> {
    hashed: HashMap<u64, Vec<Slot<T>>>,
    steps: u64,
    tick: u64,
}

impl<T: Clone + Default> TimeWheel<T> {
    // create new hashed time wheel instance
    pub fn new(steps: u64) -> Self {
        TimeWheel {
            steps: steps,
            hashed: HashMap::new(),
            tick: 0,
        }
    }

    pub fn add(&mut self, timeout: u64, value: T) {
        log::trace!(
            "add timeout({}) steps({}) tick({})",
            timeout,
            self.steps,
            self.tick
        );

        let slot = (timeout + self.tick) % self.steps;
        let round = timeout / self.steps;

        log::trace!(
            "add timeout({}) to slot({}) with round({}), current tick is {}",
            timeout,
            slot,
            round,
            self.tick
        );

        let slots = self.hashed.entry(slot).or_insert(Vec::new());

        slots.push(Slot { t: value, round });
    }

    pub fn tick(&mut self) -> Poll<Vec<T>> {
        let step = self.tick % self.steps;

        self.tick += 1;

        if let Some(slots) = self.hashed.remove(&step) {
            let mut current: Vec<T> = vec![];
            let mut reserved: Vec<Slot<T>> = vec![];

            for slot in slots {
                if slot.round == 0 {
                    current.push(slot.t);
                } else {
                    reserved.push(Slot::<T> {
                        t: slot.t,
                        round: slot.round - 1,
                    });
                }
            }

            if !reserved.is_empty() {
                self.hashed.insert(step, reserved);
            }

            if !current.is_empty() {
                return Poll::Ready(current);
            }
        }

        Poll::Pending
    }
}
