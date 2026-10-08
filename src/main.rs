const READY: u64 = 1 << 0;
const RUNNING: u64 = 1 << 1;
const COMPLETED: u64 = 1 << 2;
const FAILED: u64 = 1 << 3;
const BIG_ENDIAN: u64 = 1 << 4;
const STATUS_MASK: u64 = READY | RUNNING | COMPLETED | FAILED;
const PRIO_SHIFT: u32 = 5;
const ATTS_SHIFT: u32 = 8;
const ID_SHIFT: u32 = 16;
const MAX_ATTS: u8 = 2;

struct Job {
    name: String,
    state: u64,
}
impl Job {
    fn new(name: &str, id: u16, priority: u8) -> Job { // makes a new job with a priority, id, and name.
        assert!(priority <= 7, "priority must fit in three bits");
        let mut job = Job {
            name: String::from(name),
            state: READY | ((priority as u64) << PRIO_SHIFT) | ((id as u64) << ID_SHIFT),
        };
        job.setFlag(BIG_ENDIAN, cfg!(target_endian = "big"));
        job
    }

    fn priority(&self) -> u8 { // returns the priority
        return ((self.state >> PRIO_SHIFT) & 0b111) as u8;
    }

    fn attempts(&self) -> u8 { // counts # attempts
        let res = (self.state >> ATTS_SHIFT) & 0xff;
        return res as u8;
    }


    fn id(&self) -> u16 { // gets the id from the bits
        ((self.state >> ID_SHIFT) & 0xffff) as u16
    }
    fn setFlag(&mut self, flag: u64, enabled: bool) {  // sets the flag
        if enabled { self.state |= flag; }
         else { self.state &= !flag; }
    }
    fn hasFlag(&self, flag: u64) -> bool { // checks if it has a flag
        self.state & flag != 0
    }

    fn incrementAttempts(&mut self) { // add an attempt when tried and make sure attempts is not too big
        assert!(self.attempts() < 255, "too many attempts");
        let next = self.attempts() + 1;
        self.state &= !(0xff << ATTS_SHIFT);
        self.state |= (next as u64) << ATTS_SHIFT;
    }

    fn setStatus(&mut self, status: u64) { // set the status (ok maybe this comment was useless but I want pointers for when I'm presenting)
        self.state &= !STATUS_MASK;
        self.state |= status;
    }

    fn idBytes(&self) -> [u8; 2] { // use big or little endian depending on the endian bit
        if self.hasFlag(BIG_ENDIAN) { self.id().to_be_bytes() }
        else { self.id().to_le_bytes() }
    }

    fn show(&self) { // print depending on the status
        let mut s = "invalid";
        if self.hasFlag(FAILED) { s = "Failed"; }
        else if self.hasFlag(RUNNING) { s = "Running"; }
        else if self.hasFlag(COMPLETED) { s = "Completed"; }
        else if self.hasFlag(READY) { s = "Ready"; }

        println!("{:<16} {s:<9} priority={} attempts={} id=0x{:04X} state=0x{:016X}", self.name, self.priority(), self.attempts(), self.id(), self.state);
    }

    fn idBigEndian(&self) -> [u8; 2] { // return the id's bytes in big endian order
        let mut bytes = self.idBytes();
        if !self.hasFlag(BIG_ENDIAN) { bytes.reverse(); } // flip if the big_endian flag is absent: the source bytes are little endian
        bytes
    }
}

fn nextJob(jobs: &[Job]) -> usize {
    let mut bestJob = jobs.len(); // this means no ready job, if it is past the valid index.
    let mut top_prio = -1;
    for i in 0..jobs.len() { // this is awful for loop syntax and another experience with the compiler I did not enjoy
        if jobs[i].hasFlag(READY) {
            let prio = jobs[i].priority() as i32; // I do not like this 'as type' syntax.
            if prio > top_prio { // don't update on a tie
                top_prio = prio;
                bestJob = i;
            }
        }
    }
    return bestJob;
}

fn runNext(jobs: &mut [Job]) -> bool {
    let i = nextJob(jobs);
    if i == jobs.len() { // no jobs are ready, so we can't run another job (return false to let caller know)
        return false;
    }
    let job = &mut jobs[i];
    job.setStatus(RUNNING);
    job.incrementAttempts();
    job.show();

    // email job will fail once, then retry before backup and generate report because it has higher priority. this is a deterministic demo
    if job.name == "send_email" && job.attempts() == 1 {
        job.setStatus(FAILED);
        job.show();
        if job.attempts() < MAX_ATTS {
            job.setStatus(READY);
            job.show();
        }
    } else {
        job.setStatus(COMPLETED);
        job.show();
    }
    true
}

fn main() {
    let mut jobs = vec![Job::new("backup", 0x1234, 2), Job::new("send_email", 0x9876, 7), Job::new("generate_report", 0x6767, 4)];
    println!("The initial queue  (note: highest priority goes first, not lowest):");

    for job in &jobs { // why is this syntax a thing? another experience with the rust compiler. Was a cool trick to learn though.
        job.show();
    }

    println!("\nEndian demo for job ID 0x6769:");
    let mut ex = Job::new("endian_demo", 0x6769, 0);
    for endian_bit in [0, 1] {
        // this just overrides the bit for this example to exercise both paths. would not happen in real code. 
        ex.setFlag(BIG_ENDIAN, endian_bit == 1);
        println!("endian bit={} source={:02X?} big-endian output={:02X?}", endian_bit, ex.idBytes(), ex.idBigEndian());
    }

    println!("\nExecution:");
    while runNext(&mut jobs) {
    } // another compiler experience
    println!("\nFinal queue:");
    for job in &jobs { job.show(); }
}

#[cfg(test)]
mod tests {
    use super::*; // another compiler experience

    #[test]
    fn endianConversionHandlesAllOrders() {
        let mut job = Job::new("test", 0x6769, 0);
        job.setFlag(BIG_ENDIAN, false);
        assert_eq!(job.idBytes(), [0x69, 0x67]);
        assert_eq!(job.idBigEndian(), [0x67, 0x69]);

        job.setFlag(BIG_ENDIAN, true);
        assert_eq!(job.idBytes(), [0x67, 0x69]);
        assert_eq!(job.idBigEndian(), [0x67, 0x69]);
    }

    #[test]
    fn schedulerOrdersRetriesAndFinishesIfFailedOnFirstTry() {
        let mut jobs = vec![Job::new("backup", 1, 2), Job::new("send_email", 2, 7)];
        assert_eq!(nextJob(&jobs), 1);
        assert!(runNext(&mut jobs));
        assert!(jobs[1].hasFlag(READY));

        assert!(runNext(&mut jobs));
        assert!(jobs[1].hasFlag(COMPLETED));
        assert_eq!(jobs[1].attempts(), 2);

        assert_eq!(nextJob(&jobs), 0);
        assert!(runNext(&mut jobs));
        assert!(!runNext(&mut jobs));

        let tied = vec![Job::new("first", 1, 3), Job::new("second", 2, 3)];
        assert_eq!(nextJob(&tied), 0);

        let mut empty = vec![];
        assert!(!runNext(&mut empty));
    }

    #[test]
    fn fieldsSurviveStatusAttemptChanges() {
        let mut job = Job::new("test", 0xffff, 7);
        job.setFlag(BIG_ENDIAN, true);
        for _ in 0..255 {
            job.incrementAttempts();
        }
        job.setStatus(COMPLETED);
        assert_eq!((job.id(), job.priority(), job.attempts()), (0xffff, 7, 255));

        assert_eq!(job.state & STATUS_MASK, COMPLETED);
        assert!(job.hasFlag(BIG_ENDIAN));
        assert_eq!(job.state >> 32, 0);
    }
}
