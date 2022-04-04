use std::arch::x86_64::_mm_pause;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::windows::fs::FileExt;
use std::thread::{sleep, yield_now};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, std::io::Error>;

fn main() -> Result<()> {
    let env: Vec<String> = std::env::args().collect();
    if env.len() < 2 {
        println!(
            "Enter the FilePath, to write the time to as a argument {:#?}",
            env
        );
        return Ok(());
    }
    let filepath = unsafe { env.get_unchecked(1) };
    //Safety: Checked above
    println!(
        "The contents of the following file will be deleted.
It will be used for this program, to write the current system time to.
'{}'",
        filepath
    );
    pause();
    let mut test = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(filepath)?;
    let mut times = 0;
    loop {
        let time = std::time::SystemTime::now();
        let s = Instant::now();
        let dur = time
            .duration_since(std::time::UNIX_EPOCH)
            .expect("The clock was set to before 1970-01-01 00:00:00. Please set your clock.");
        let seconds = dur.as_secs() % 60;
        let minutes = dur.as_secs() / 60 % 60;
        let hours = dur.as_secs() / 60 / 60 % 24;
        let time = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);
        let r = test.seek(SeekFrom::Start(0));
        if r.is_err() {
            let r1 = test.seek_write(time.as_bytes(), 0);
            if r.is_err() {
                eprintln!(
                    "Could not seek, or seek and write.\n\
				Error for seek is {}. Error for seek and write is {}.",
                    r.unwrap_err(),
                    r1.unwrap_err()
                );
            }
        } else {
            let r = test.write_all(time.as_bytes());
            if r.is_err() {
                eprintln!(
                    "Could not write to file. File write returned {}",
                    r.unwrap_err()
                );
            }
        }
        times += 1;
        let dur_no_sec = dur - Duration::from_secs(dur.as_secs());
        if (dur_no_sec + s.elapsed()).as_millis() > 100 && times > 10 {
            panic!(
                "Something went wrong. We wrote {}ms after the second changed.",
                (dur_no_sec + s.elapsed()).as_millis()
            );
        }
        println!(
            "{}ms slow, processing & writing took {}ns",
            dur_no_sec.as_millis(),
            s.elapsed().as_nanos()
        );
        sleep(Duration::from_millis(1000) - dur_no_sec - s.elapsed());
    }
}

fn pause() {
    println!("Press any key to continue...");
    let clin = std::io::stdin();
    let mut str = "".to_string();
    clin.read_line(&mut str).unwrap();
    println!("Resuming.");
}