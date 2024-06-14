use std::fmt::{Display, Formatter};
use std::io::SeekFrom;
use std::ops::Add;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use chrono::Timelike;
use egui::{Response, Ui, Widget};
use rfd::FileHandle;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};
use crate::app::popup;
use crate::app::popup::{handle_display_popup_arc, popup_creator};
use crate::get_runtime;

const SECONDS_IN_MINUTE: u64 = 60;
const MINUTES_IN_HOUR: u64 = 60;
const HOURS_IN_DAY: u64 = 24;
const SECONDS_IN_DAY: u64 = SECONDS_IN_MINUTE * MINUTES_IN_HOUR * HOURS_IN_DAY;
fn sec_to_timestamp(sec: i64) -> (u64, u64, u64, bool){
    let neg = sec.is_negative();
    let sec = sec.unsigned_abs();
    let s = sec%SECONDS_IN_MINUTE;
    let m = sec/SECONDS_IN_MINUTE;
    let h = m/MINUTES_IN_HOUR;
    let m  = m%MINUTES_IN_HOUR;
    (s, m, h, neg)
}

const MODES:&[Mode] = &[Mode::Counter, Mode::Timer, Mode::SystemTime];

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum Mode {
    //Counts up towards infinity
    #[default]
    Counter,
    //Counts Down
    Timer,
    //Matches system time
    SystemTime,
}

impl Mode{
    pub const fn get_desc(self) -> &'static str{
        match self{
            Self::Counter => "Time to Start Counting Up from:",
            Self::Timer => "Time to Start Counting Down from:",
            Self::SystemTime => "Time to add to Current Local Time:",
        }
    }
    pub fn get_timestamp(self, s: &Arc<AtomicI64>, mut start_sec: i64, overall_change: Duration) -> (u64, u64, u64, bool, bool){
        match self{
            Self::Timer =>  {
                let dur = start_sec.checked_sub_unsigned(overall_change.as_secs());
                let maxed = dur.is_none();
                let dur = dur.unwrap_or(i64::MIN);
                s.store(dur, Ordering::Release);
                let (s, m, h, neg) = sec_to_timestamp(dur);
                (s, m, h, neg, maxed)
            },
            Self::Counter => {
                let dur = start_sec.checked_add_unsigned(overall_change.as_secs());
                let maxed = dur.is_none();
                let dur = dur.unwrap_or(i64::MAX);
                s.store(dur, Ordering::Release);
                let (s, m, h, neg) = sec_to_timestamp(dur);
                (s, m, h, neg, maxed)
            },
            Self::SystemTime => {
                start_sec = start_sec % SECONDS_IN_DAY as i64;
                debug_assert!(chrono::TimeDelta::try_seconds(SECONDS_IN_DAY as i64).is_some());
                let timedelta = chrono::TimeDelta::try_seconds(start_sec).unwrap_or_else(||{
                    log::error!("How did we get here? Apparently {SECONDS_IN_DAY}*1000>{0} or -{SECONDS_IN_DAY}*1000 > -{0}", i64::MAX);
                    chrono::TimeDelta::nanoseconds(0)
                });
                let time = chrono::Local::now().time().overflowing_add_signed(timedelta).0;
                (u64::from(time.second()), u64::from(time.minute()), u64::from(time.hour()), false, false)
            },
        }
    }

}

impl Display for Mode{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Counter => write!(f, "Counter (Up)"),
            Self::Timer => write!(f, "Timer (Down)"),
            Self::SystemTime => write!(f, "SystemTime"),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct CounterTimer {
    pub name: Arc<str>,
    mode: Mode,
    file: PathBuf,
    time_s: Arc<AtomicI64>,
    #[serde(skip)]
    file_pick: Option<JoinHandle<Option<FileHandle>>>,
    #[serde(skip)]
    counter: Option<(tokio::sync::oneshot::Sender<()>, JoinHandle<()>)>,
    #[serde(skip)]
    pub(crate) popup: crate::app::popup::ArcPopupStore
}

impl CounterTimer {
    pub(crate) fn new(name: Arc<str>, popup: crate::app::popup::ArcPopupStore) -> Self {
        Self{
            name,
            mode: Mode::default(),
            file: Default::default(),
            time_s: Arc::new(AtomicI64::new(0)),
            file_pick: None,
            counter: None,
            popup,
        }
    }

    fn check_counter(&mut self){
        if let Some((sender, handle)) = self.counter.take(){
            if handle.is_finished(){
                match get_runtime().block_on(handle) {
                    Ok(()) => {}
                    Err(err) => {
                        log::error!("Counter Thread Paniced: {err}");
                        handle_display_popup_arc(
                            &self.popup,
                            "An internal Error occurred",
                            &err,
                            "Error in Timer"
                        );
                    }
                }
            }else{
                self.counter = Some((sender, handle))
            }
        }
    }
    fn check_file_pick(&mut self) {
        if let Some(task) = self.file_pick.take() {
            if task.is_finished(){
                match get_runtime().block_on(task){
                    Ok(Some(ok)) => {
                        self.file = ok.path().to_path_buf();
                    }
                    Ok(None) => {
                        log::info!("No File Selected.");
                        popup_creator(
                            self.popup.clone(),
                            "No File Picked",
                            |_, ui,_,_|{
                                ui.label("Not considering File Selection as no file was picked");
                            }
                        )
                    }
                    Err(err) => {
                        log::error!("Panic whilst picking File: {err}");
                        popup::handle_display_popup_arc(
                            &self.popup,
                            "A critical internal app error occurred whilst picking a File",
                            &err,
                            "Critical error whilst picking File"
                        )
                    }
                }
            }else{
                self.file_pick = Some(task);
            }
        }
    }
    pub fn stop_counter(&mut self){
        self.check_counter();
        if let Some((sender,_)) = self.counter.take() {
            match sender.send(()) {
                Ok(()) => {}
                Err(()) => {
                    log::info!("Counter {} has already exited early?", self.name.as_ref());
                }
            }
        }
    }
    pub fn start_counter(&mut self){
        if self.counter.is_some() {return;}
        let mode = self.mode;
        let file = self.file.clone();
        let s = self.time_s.clone();
        let start_s = self.time_s.load(Ordering::Acquire);
        let popups = self.popup.clone();
        let (send, mut recv) = tokio::sync::oneshot::channel();
        let thread = tokio::spawn(async move {
            let mut last_message = None;
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(file.as_path())
                .await
                .map_or_else(|err|{
                    log::error!("Failed opening file: {err}");
                    crate::app::popup::handle_display_popup_arc(
                        &popups,
                        "Failed to get file to write the counter time to",
                        &err,
                        "Failed opening File"
                    );
                    None
                }, |ok| Some(ok));
            let start_instant = Instant::now();
            let mut interval = tokio::time::interval_at(start_instant.add(crate::PERIOD), crate::PERIOD);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop{
                tokio::select! {
                    biased;
                    _ = &mut recv => {
                        break
                    }
                    test = interval.tick() => {
                        let overall_change = test - start_instant;
                        let (s, m, h, neg, maxed) = mode.get_timestamp(&s, start_s, overall_change);
                        if let Some(file) = file.as_mut() {
                            if let Err(err) = file.seek(SeekFrom::Start(0)).await {
                                if last_message.map_or(true, |insant: Instant|insant.elapsed().as_secs() > crate::NOTIFICATION_TIMEOUT){
                                    last_message = Some(Instant::now());
                                    log::error!("Error moving Cursor: {err}");
                                    crate::app::popup::handle_display_popup_arc(
                                        &popups,
                                        "Could not make next write overwrite file",
                                        &err,
                                        "Error Seeking"
                                    );
                                }
                            }else{
                                if let Err(err) = file.write_all(format!("{0}{h:02}:{m:02}:{s:02}", if neg {"-"} else {""}).as_bytes()).await {
                                    if last_message.map_or(true, |insant: Instant|insant.elapsed().as_secs() > crate::NOTIFICATION_TIMEOUT){
                                        last_message = Some(Instant::now());
                                        log::error!("Error writing to File: {err}");
                                        crate::app::popup::handle_display_popup_arc(
                                            &popups,
                                            "Could not write to file",
                                            &err,
                                            "Error Writing"
                                        );
                                    }
                                }
                            }
                        }
                        if maxed {
                            handle_display_popup_arc(
                                &popups,
                                "A Timer has reached it's limits due to limitations of Computers",
                                &"The Numeric Representation of the Timer in Seconds would overflow a signed 64-bit integer.",
                                "Reached timer limit",
                            );
                            break;
                        }
                    }
                }
            }
        });
        self.counter = Some((send, thread));
    }
}

impl Widget for &mut CounterTimer{
    fn ui(self, ui: &mut Ui) -> Response {
        self.check_counter();
        self.check_file_pick();
        ui.vertical(|ui|{
            ui.horizontal(|ui |{
                ui.label("Current File: ");
                ui.label(self.file.to_string_lossy());
                if ui.button("Select File").clicked(){
                    if let Some(picker) = self.file_pick.take(){
                        picker.abort();
                    }
                    self.file_pick = Some(tokio::spawn(rfd::AsyncFileDialog::default().set_directory(".").pick_file()));
                }
            });

            ui.horizontal(|ui | {
                if self.counter.is_none() {
                    if ui.button(format!("Start {}", self.mode)).clicked() {
                        self.start_counter();
                    }
                } else {
                    if ui.button(format!("Stop {}", self.mode)).clicked() {
                        self.stop_counter();
                    }
                }
                ui.add_enabled_ui(self.counter.is_none(), |ui| {
                    egui::ComboBox::new(self.name.as_ref(), "")
                        .selected_text(self.mode.to_string())
                        .show_ui(
                            ui,
                            |ui| for mode in MODES {
                                ui.selectable_value(&mut self.mode, *mode, mode.to_string());
                            }
                        );
                });
            });

            ui.add_enabled_ui(self.counter.is_none(), |ui|{
                ui.horizontal(|ui|{
                    ui.label(self.mode.get_desc());
                    let mut s = self.time_s.load(Ordering::Acquire);
                    egui::DragValue::new(&mut s)
                        .custom_formatter(|mut sec,_|{
                            let neg = sec.is_sign_negative();
                            if neg {
                                sec = -sec;
                            }
                            let s = sec.rem_euclid(SECONDS_IN_MINUTE as f64);
                            let min = sec.div_euclid(SECONDS_IN_MINUTE as f64);
                            let hr = min.div_euclid(MINUTES_IN_HOUR as f64);
                            let min  = min.rem_euclid(MINUTES_IN_HOUR as f64);
                            format!("{0}{hr:02.0}:{min:02.0}:{s:02.0}", if neg {"-"} else {""})
                        }).custom_parser(|string|{
                            let neg = string.strip_prefix("-");
                            let vec = neg.unwrap_or(string).rsplit(":").collect::<Vec<_>>();
                            let mut seconds = 0.;
                            let mut conversion = 1.;
                            for (loops, string) in vec.iter().enumerate() {
                                seconds += conversion * f64::from_str(string).unwrap_or(0.);
                                if loops < 3 {
                                    //Hours->Minutes && Minutes->Seconds
                                    conversion *= 60.;
                                } else if loops == 4 {
                                    //Days->Hours
                                    conversion *= 24.;
                                } else if loops == 4 {
                                    //Months->Days
                                    //365.2425 is the average Year length in Days of the Gregorian calendar
                                    conversion *= 365.2425/12.;
                                } else if loops == 5 {
                                    //Year->Months
                                    //365.2425 is the average Year length in Days of the Gregorian calendar
                                    conversion *= 12.;
                                } else {
                                    //I don't care beyond this point. tbh everything past days is already extra
                                    conversion *= 1000.;
                                }
                            }
                            if neg.is_some() {
                                seconds = -seconds;
                            }
                            Some(seconds)
                        }).ui(ui);
                    self.time_s.store(s, Ordering::Release);
                })
            })
        }).response
    }
}