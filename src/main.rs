const READY: u64 = 1 << 0;
const RUNNING: u64 = 1 << 1;
const COMPLETED: u64 = 1 << 2;
const FAILED: u64 = 1 << 3;
const BIG_ENDIAN: u64 = 1 << 4;
const STATUS_MASK: u64 = READY | RUNNING | COMPLETED | FAILED;
const PRIORITY_SHIFT: u32 = 5;
const ATTEMPTS_SHIFT: u32 = 8;
const ID_SHIFT: u32 = 16;
const MAX_ATTEMPTS: u8 = 2;

struct Job {
    name: String,
    state: u64,
}

impl Job {
    fn new(name: &str, id: u16, priority: u8) -> Job {
        assert!(priority <= 7, "priority must fit in three bits");
        let mut job = Job {
            name: String::from(name),
            state: READY | ((priority as u64) << PRIORITY_SHIFT) | ((id as u64) << ID_SHIFT),
        };
        job.set_flag(BIG_ENDIAN, cfg!(target_endian = "big"));
        job
    }

    fn has_flag(&self, flag: u64) -> bool {
        self.state & flag != 0
    }

    fn set_flag(&mut self, flag: u64, enabled: bool) {
        if enabled {
            self.state |= flag;
        } else {
            self.state &= !flag;
        }
    }

    fn priority(&self) -> u8 {
        ((self.state >> PRIORITY_SHIFT) & 0b111) as u8
    }

    fn attempts(&self) -> u8 {
        ((self.state >> ATTEMPTS_SHIFT) & 0xff) as u8
    }

    fn id(&self) -> u16 {
        ((self.state >> ID_SHIFT) & 0xffff) as u16
    }

    fn increment_attempts(&mut self) {
        assert!(self.attempts() < 255, "attempt count overflow");
        let next = self.attempts() + 1;
        self.state &= !(0xff << ATTEMPTS_SHIFT);
        self.state |= (next as u64) << ATTEMPTS_SHIFT;
    }

    fn set_status(&mut self, status: u64) {
        // Exactly one lifecycle flag is set; other fields are preserved.
        self.state &= !STATUS_MASK;
        self.state |= status;
    }

    fn id_bytes(&self) -> [u8; 2] {
        if self.has_flag(BIG_ENDIAN) {
            self.id().to_be_bytes()
        } else {
            self.id().to_le_bytes()
        }
    }

    fn id_big_endian(&self) -> [u8; 2] {
        let mut bytes = self.id_bytes();
        // Normalize the simulated machine's byte order to network byte order.
        if !self.has_flag(BIG_ENDIAN) {
            bytes.reverse();
        }
        bytes
    }

    fn show(&self) {
        let mut status = "invalid";
        if self.has_flag(READY) {
            status = "ready";
        } else if self.has_flag(RUNNING) {
            status = "running";
        } else if self.has_flag(COMPLETED) {
            status = "completed";
        } else if self.has_flag(FAILED) {
            status = "failed";
        }
        println!(
            "{:<16} {status:<9} priority={} attempts={} id=0x{:04X} state=0x{:016X}",
            self.name, self.priority(), self.attempts(), self.id(), self.state
        );
    }
}

fn next_job(jobs: &[Job]) -> usize {
    // jobs.len() means "no ready job": it is past the last valid index.
    let mut best = jobs.len();
    let mut highest_priority = -1;
    for i in 0..jobs.len() {
        if jobs[i].has_flag(READY) {
            let priority = jobs[i].priority() as i32;
            // Using > keeps the first queued job when priorities tie.
            if priority > highest_priority {
                best = i;
                highest_priority = priority;
            }
        }
    }
    best
}

fn run_next(jobs: &mut [Job]) -> bool {
    let index = next_job(jobs);
    if index == jobs.len() {
        return false;
    }
    let job = &mut jobs[index];
    job.set_status(RUNNING);
    job.increment_attempts();
    job.show();

    // Deterministic demo: the email job fails once, then succeeds on retry.
    if job.name == "send_email" && job.attempts() == 1 {
        job.set_status(FAILED);
        job.show();
        if job.attempts() < MAX_ATTEMPTS {
            job.set_status(READY);
            job.show();
        }
    } else {
        job.set_status(COMPLETED);
        job.show();
    }
    true
}

fn main() {
    let mut jobs = vec![
        Job::new("backup", 0x1234, 2),
        Job::new("send_email", 0x5678, 7),
        Job::new("generate_report", 0x9abc, 4),
    ];
    println!("Initial queue (higher priority runs first):");
    for job in &jobs {
        job.show();
    }

    println!("\nEndian demo for job ID 0x1234:");
    let mut example = Job::new("endian_demo", 0x1234, 0);
    for endian_bit in 0..2 {
        // Override only this example's machine bit to exercise both paths.
        example.set_flag(BIG_ENDIAN, endian_bit == 1);
        println!(
            "endian bit={} source={:02X?} big-endian output={:02X?}",
            endian_bit, example.id_bytes(), example.id_big_endian()
        );
    }

    println!("\nExecution:");
    while run_next(&mut jobs) {}
    println!("\nFinal queue:");
    for job in &jobs {
        job.show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_survive_status_and_attempt_changes() {
        let mut job = Job::new("test", 0xffff, 7);
        job.set_flag(BIG_ENDIAN, true);
        for _ in 0..255 {
            job.increment_attempts();
        }
        job.set_status(COMPLETED);
        assert_eq!((job.id(), job.priority(), job.attempts()), (0xffff, 7, 255));
        assert_eq!(job.state & STATUS_MASK, COMPLETED);
        assert!(job.has_flag(BIG_ENDIAN));
        assert_eq!(job.state >> 32, 0);
    }

    #[test]
    fn endian_conversion_handles_both_orders() {
        let mut job = Job::new("test", 0x1234, 0);
        job.set_flag(BIG_ENDIAN, false);
        assert_eq!(job.id_bytes(), [0x34, 0x12]);
        assert_eq!(job.id_big_endian(), [0x12, 0x34]);
        job.set_flag(BIG_ENDIAN, true);
        assert_eq!(job.id_bytes(), [0x12, 0x34]);
        assert_eq!(job.id_big_endian(), [0x12, 0x34]);
    }

    #[test]
    fn scheduler_orders_retries_and_finishes() {
        let mut jobs = vec![Job::new("backup", 1, 2), Job::new("send_email", 2, 7)];
        assert_eq!(next_job(&jobs), 1);
        assert!(run_next(&mut jobs));
        assert!(jobs[1].has_flag(READY));
        assert!(run_next(&mut jobs));
        assert!(jobs[1].has_flag(COMPLETED));
        assert_eq!(jobs[1].attempts(), 2);
        assert_eq!(next_job(&jobs), 0);
        assert!(run_next(&mut jobs));
        assert!(!run_next(&mut jobs));
        let tied = vec![Job::new("first", 1, 3), Job::new("second", 2, 3)];
        assert_eq!(next_job(&tied), 0);
        let mut empty = vec![];
        assert!(!run_next(&mut empty));
    }
}
