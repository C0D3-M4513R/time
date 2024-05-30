use std::fmt::{Display, Formatter};
use std::io::SeekFrom;
use std::ops::Add;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use egui::{Response, Ui, Widget};
use rfd::FileHandle;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};
use crate::app::popup;
use crate::app::popup::{handle_display_popup_arc, popup_creator};
use crate::get_runtime;

fn duration_to_timestamp(dur:Duration) -> (u32, u64, u64, u64){
    let mut s = dur.as_secs();
    let mut m = s/60;
    s %= 60;
    let h = m/60;
    m %= 60;
    (dur.subsec_nanos(), s, m, h)
}

const MODES:&[Mode] = &[Mode::Counter, Mode::Timer];

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum Mode {
    //Counts up towards infinity
    #[default]
    Counter,
    //Counts Down
    Timer,
}

impl Display for Mode{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Counter => write!(f, "Counter (Up)"),
            Mode::Timer => write!(f, "Timer (Down)"),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct CounterTimer {
    pub name: Arc<str>,
    mode: Mode,
    file: PathBuf,
    time_s: Arc<AtomicU64>,
    #[serde(skip)]
    file_pick: Option<JoinHandle<Option<FileHandle>>>,
    #[serde(skip)]
    counter: Option<(tokio::sync::oneshot::Sender<()>, JoinHandle<()>)>,
    #[serde(skip)]
    popup: crate::app::popup::ArcPopupStore
}



impl CounterTimer {
    pub(crate) fn new(name: Arc<str>, popup: crate::app::popup::ArcPopupStore) -> Self {
        Self{
            name,
            mode: Mode::default(),
            file: Default::default(),
            time_s: Arc::new(AtomicU64::new(0)),
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
        let start_dur = Duration::new(self.time_s.load(Ordering::Acquire), 0);
        let popups = self.popup.clone();
        let (send, mut recv) = tokio::sync::oneshot::channel();
        let thread = tokio::spawn(async move {
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
                        let dur = match mode {
                            Mode::Timer => if overall_change > start_dur {
                                break
                            }else{
                                start_dur - overall_change
                            },
                            Mode::Counter => start_dur + overall_change
                        };
                        s.store(dur.as_secs(), Ordering::Release);
                        let (_ns, s, m, h) = duration_to_timestamp(dur);
                        if let Some(file) = file.as_mut() {
                            if let Err(err) = file.seek(SeekFrom::Start(0)).await {
                                log::error!("Error moving Cursor: {err}");
                                crate::app::popup::handle_display_popup_arc(
                                    &popups,
                                    "Could not make next write overwrite file",
                                    &err,
                                    "Error Seeking"
                                );
                                continue;
                            }
                            if let Err(err) = file.write_all(format!("{h:02}:{m:02}:{s:02}").as_bytes()).await {
                                log::error!("Error writing to File: {err}");
                                crate::app::popup::handle_display_popup_arc(
                                    &popups,
                                    "Could not write to file",
                                    &err,
                                    "Error Writing"
                                );
                                continue;
                            }
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

            if self.counter.is_none(){
                if ui.button(format!("Start {}", self.mode)).clicked(){
                    self.start_counter();
                }
            }else{
                if ui.button(format!("Stop {}", self.mode)).clicked(){
                    self.stop_counter();
                }
            }

            ui.add_enabled_ui(self.counter.is_none(), |ui|{
                egui::ComboBox::new(self.name.as_ref(), "")
                    .selected_text(self.mode.to_string())
                    .show_ui(
                        ui,
                        |ui|for mode in MODES{
                            ui.selectable_value(&mut self.mode, *mode, mode.to_string());
                        }
                    );
                ui.horizontal(|ui|{
                    let (_, mut s, mut m, mut h) = duration_to_timestamp(Duration::new(self.time_s.load(Ordering::Acquire), 0));
                    egui::DragValue::new(&mut h).custom_formatter(|n,_|format!("{n:02.0}")).ui(ui);
                    ui.label(":");
                    egui::DragValue::new(&mut m).custom_formatter(|n,_|format!("{n:02.0}")).ui(ui);
                    ui.label(":");
                    egui::DragValue::new(&mut s).custom_formatter(|n,_|format!("{n:02.0}")).ui(ui);
                    self.time_s.store(s+(m+h*60)*60, Ordering::Release);
                })
            })
        }).response
    }
}