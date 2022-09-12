extern crate core;

use std::fs::OpenOptions;
use std::io::Write;
use std::ops::{Add, Div};
use std::path::PathBuf;
use std::str::FromStr;
use std::thread::yield_now;
use std::time::{Duration,Instant};
//use std::process::Termination;

mod error;
type Result<T,S=error::Error> = std::result::Result<T,S>;
//needs to be in 0..1000
const SLEEP_CUTOFF:u32 = 50;

#[export_name = "main_fn"]
fn main() -> Result<(),Box<dyn std::error::Error>> {
    let env: Vec<String> = std::env::args().collect();
    if env.len()<= 1 {
        println!("Correct usage is: '{} <filepath> [seconds] [minutes] [hours] [days]', where <> args are required, and [] is optional.", env.get(0).expect("No executable found in first parameter."));
        println!("Default timer length is 12h, if nothing else is specified.");
        return Ok(());
    } else if env.len() < 2 {
        println!(
            "Enter the FilePath, to write the time to as a argument {:#?}",
            env
        );
        pause();
        return Ok(());
    }else if env.len() < 3 {
        println!("No default duration set. Using 12h.");
    }
    let timer;
    {
        let mut init_timer_sec:u64=0;
        const ZERO:&'static str = "0";
        fn arg_to_num<T:FromStr>(env:&Vec<String>,n:usize, zro:T)->T{
            match T::from_str(env.get(n).unwrap_or(&ZERO.to_string()).as_str()) {
                Ok(v)=>v,
                Err(_)=>zro,
            }
        }
        //seconds
        init_timer_sec += arg_to_num(&env,2, 0);
        //minutes
        init_timer_sec +=arg_to_num(&env,3, 0)*60;
        //hours
        init_timer_sec +=arg_to_num(&env,4, 0)*60*60;
        //days
        init_timer_sec +=arg_to_num(&env,5, 0)*60*60*24;
        //init timer
        if init_timer_sec==0{
            init_timer_sec=12*60*60;//12h default.
        }
        timer=Duration::from_secs(init_timer_sec);
    }
    
    let filepath = unsafe { env.get_unchecked(1) };
    //Safety: Checked above
    println!(
        "The contents of the following file will be deleted.
It will be used for this program, to write the current system time to.
'{}'",
        filepath
    );
    write_replace(PathBuf::from(filepath),"".to_string());
    let filepath= std::fs::canonicalize(filepath).expect("Filepath not valid");
    
    pause();
    #[cfg(debug_assertions)]
    {
        let s = new_instant()?;
        println!("The output of the next line may be wierd. Ignore if you don't understand it.");
        println!("Start-Time is around {:#?}, and End-Time should be around {:#?}",s,s.checked_add(timer));
        println!("Times may vary, according to timezone, however the relative distance should be the same.");
    }
    timer_loop(timer,filepath);
    #[allow(unreachable_code)]
    {
        println!("Timer is done");
        pause();
        Ok(())
    }
}


// #[no_panic::no_panic]
// #[no_mangle]
//For now this doesn't stop. The timer keeps going.
fn timer_loop(duration:Duration,filepath:PathBuf)->!{
    let start = Instant::now();
    let end = start.add(duration);
    #[cfg(debug_assertions)]
    println!("Actual start time is {:#?} and end time is {:#?}", start, end);

    let inner_loop = ||->Result<()>{
        let time = sleep(start)?;
        #[cfg(debug_assertions)]
        let s = new_instant()?;
        let mut neg = false;
        let local =  match end.checked_duration_since(time){
            Some(v)=>v,
            None=>{neg=true;time.checked_duration_since(end).ok_or(error::Error::InstantAdd)?},
        };
        #[cfg(debug_assertions)]
        {
            println!("lost {}ms to sleep inaccuracies",local.subsec_millis());
        }
        let seconds = local.as_secs() % 60;
        let minutes = local.as_secs() / 60 % 60;
        let hours = local.as_secs() / 60 / 60 % 24;
        
        let time = format!("{}{:02}:{:02}:{:02}",if neg {"-"} else {""}, hours, minutes, seconds);
        #[cfg(debug_assertions)]
        let w = new_instant()?;
        #[cfg(debug_assertions)]
        {
            println!("Got String to write after {}ys", s.elapsed().as_micros());
        }
        if let Err(_) = std::panic::catch_unwind(||write_replace(filepath.clone(),time)){
            //We cannot handle this error anyways, if the os won't let us. Let this just fly by.
            std::io::stderr().write_all(b"Could not write to file.").ok();
        }
        
        
        #[cfg(debug_assertions)]
        {
            let dur_subsec_millis = local.subsec_millis();
            println!("Writing took {}ys, processing time was {}ys and together that is {}ys", w.elapsed().as_micros(), w.duration_since(s).as_micros(), s.elapsed().as_micros());
            if dur_subsec_millis as u128 + s.elapsed().as_millis() > 100 {
                eprintln!(
                    "Something went wrong. We wrote {}ms after the second changed.",
                    dur_subsec_millis as u128 + s.elapsed().as_millis()
                );
            }
            let sleep_dur=Duration::from_millis(1000).checked_sub(Duration::from_millis(dur_subsec_millis as u64)).and_then(|x|x.checked_sub(s.elapsed())).unwrap_or(Duration::new(0,0));
            println!(
                "{}ms slow, processing & writing took {}ys. sleeping for {}ms",
                dur_subsec_millis,
                s.elapsed().as_nanos().div(&1000),
                sleep_dur.as_millis()
            );
        }
        Ok(())
        
        // sleep(
        //     Duration::from_millis(1000)
        //         - Duration::from_millis(dur_subsec_millis as u64)
        //         - s.elapsed()
        // );
    };
    loop{
        //just ignore EVERYTHING. We want stability
        std::panic::catch_unwind(inner_loop).ok();
    }
}
fn write_replace(filepath:PathBuf, time:String){
    use std::io::{Seek, SeekFrom};
    #[cfg(target_os = "windows")]
    use std::os::windows::fs::FileExt;
    let file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(filepath);
    let mut file = match file{
        Err(e) => {std::io::stderr().write_all(e.to_string().as_bytes()).ok();return},
        Ok(v)=>v,
    };
    let r = file.seek(SeekFrom::Start(0));
    if let Err(er)=r {
        eprintln!(
            "Could not seek. Error for seek is {}. ",
            er
        );
        #[cfg(target_os = "windows")]
        {
            let r1 = file.seek_write(time.as_bytes(), 0);
            if let Err(er1) = r1{
                eprintln!(
                    "Could not seek, or seek and write.\n\
                    Error for seek is {}. Error for seek and write is {}.",
                    er,
                    er1
                );
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            eprintln!(
                "Could not seek then write.\n\
                Error for seek is {}.",
                er
            );
        }
    } else {
        let r = file.write_all(time.as_bytes());
        if let Err(e) = r {
            eprintln!(
                "Could not write to file. File write returned {}",
                e
            );
        }
    }
}

// #[no_mangle]
fn sleep(earlier:Instant)->Result<Instant,error::Error>{
    let sec =new_instant()?.checked_duration_since(earlier).ok_or(error::Error::InstantAdd)?.as_secs();
    let inner_loop = ||{
        let s = new_instant()?;
        let local = s.checked_duration_since(earlier).ok_or(Some(error::Error::InstantAdd))?;
        if local.subsec_millis()<SLEEP_CUTOFF{
            std::thread::sleep(Duration::from_millis((1000-SLEEP_CUTOFF-local.subsec_millis()) as u64));
        }
        if local.subsec_millis() == 0 && !local.is_zero(){
            return Ok(s);
        }else if local.as_secs()>sec && !local.is_zero() {
            return Ok(s);
        }
        std::hint::spin_loop();
        // yield_now();
        Err(None)
    };
    loop {
        match std::panic::catch_unwind(inner_loop){
            Ok(Ok(v)) => return Ok(v),
            Ok(Err(Some(v))) => return Err(v),
            Ok(Err(None)) => (),
            Err(_)=>(),
        }
    }
}
#[inline(never)]
fn new_instant() -> Result<Instant>{
    std::panic::catch_unwind(||Instant::now()).or_else(|_|Err(error::Error::CreateInstant))
}



// #[no_mangle]
fn pause() {
    println!("Press any key to continue...");
    let clin = std::io::stdin();
    let mut str = "".to_string();
    clin.read_line(&mut str).ok();
    println!("Resuming.");
}