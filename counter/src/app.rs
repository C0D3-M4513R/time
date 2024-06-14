pub(crate) mod popup;

use std::ops::{Index, IndexMut};
use std::sync::Arc;
use std::time::Duration;
use eframe::{Frame, Storage};
use eframe::emath::Align;
use egui::{Context, Layout, Widget};
use egui::ahash::HashMap;
use egui_extras::Column;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use crate::counter_or_timer::CounterTimer;
use crate::get_runtime;

const LINK_LATEST:&str = "https://github.com/C0D3-M4513R/time/releases/latest";
const CURRENT_VERSION:&str = "-\tCurrent Version: v0.2.2";

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct App{
    next_name: String,
    names: Vec<Arc<str>>,
    counters: HashMap<Arc<str>, CounterTimer>,
    #[serde(skip)]
    other_app_state: OtherAppState,
}
#[derive(Default)]
struct OtherAppState{
    popup: popup::ArcPopupStore,
    text_err: Option<(&'static str, Instant)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let mut slf;
        if let Some(Some(state)) = cc.storage.map(|storage|storage.get_string(eframe::APP_KEY)) {
            slf = serde_json::from_str(state.as_str()).unwrap_or_else(|err| {
                let slf = Self::default();
                log::error!("Failed deserialising App State. Will reset to the defaults. Error: {err}");
                popup::handle_display_popup_arc(
                    &slf.other_app_state.popup,
                    "The App State has been reset to the defaults",
                    &err,
                    "Failed to Load App State."
                );
                slf
            });
        } else{
            log::info!("Either no storage source or no stored app state");
            slf = Self::default();
        }

        for counter in slf.counters.values_mut(){
            counter.popup = slf.other_app_state.popup.clone();
        }

        slf
    }

    fn save_custom(&self, storage: &mut dyn Storage) {
        match serde_json::to_string(self){
            Ok(state) => {
                storage.set_string(eframe::APP_KEY, state);
                log::debug!("Saved app state");
            },
            Err(err) => {
                log::warn!("Failed serialising App State. Some App changes will be lost next start. Error: {err}");
            }
        }
    }

    fn display_popups(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame){
        let old_popup = core::mem::take(
            //Speed: there should never be a long lock on the popups. (only to push basically)
            &mut *get_runtime().block_on(
                self.other_app_state.popup.lock()
            )
        );
        //we intentionally release the lock here. otherwise self would partly be borrowed, which will disallow the popup closure call
        let mut new_popup = old_popup.into_iter().filter_map(|mut popup|{
            if popup(self, ctx, frame) {
                Some(popup)
            }else{
                None
            }
        }).collect();
        //Speed: there should never be a long lock on the popups. (only to push basically)
        let mut lock = get_runtime().block_on(self.other_app_state.popup.lock());
        core::mem::swap(&mut *lock, &mut new_popup);
        lock.append(&mut new_popup);
        drop(lock);
    }
}

impl eframe::App for App{
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        ctx.request_repaint_after(crate::PERIOD);
        let default_fn= |name|{
            CounterTimer::new(name, self.other_app_state.popup.clone())
        };
        egui::CentralPanel::default().show(ctx, |ui|{
            ui.horizontal(|ui|{
                let text_resp = ui.text_edit_singleline(&mut self.next_name);
                if ui.button("Add new Counter").clicked(){
                    let name:Arc<str> = Arc::from(core::mem::take(&mut self.next_name));
                    if !self.names.contains(&name) {
                        self.other_app_state.text_err = None;
                        self.names.push(name.clone());
                        self.counters.insert(name.clone(), default_fn(name));
                    }else{
                        self.other_app_state.text_err = Some(("This name is already taken. Please provide a uniqe name.", Instant::now()));
                    }
                }
                if let Some((err_text, time)) = self.other_app_state.text_err {
                    text_resp.ctx.debug_painter()
                        .error(text_resp.rect.left_bottom(), err_text);
                    if time.elapsed().as_secs() > crate::NOTIFICATION_TIMEOUT {
                        self.other_app_state.text_err = None;
                    }
                }
                if ui.button("Start Everything").clicked(){
                    for i in self.counters.values_mut(){
                        i.start_counter();
                    }
                }
                if ui.button("Stop Everything").clicked(){
                    for i in self.counters.values_mut(){
                        i.stop_counter();
                    }
                }
            });
            ui.with_layout(Layout::bottom_up(Align::Min), |ui|{
                ui.horizontal(|ui|{
                    ui.hyperlink_to("The releases and the Source Code can be found on Github.", LINK_LATEST);
                    ui.label(CURRENT_VERSION);
                });
                ui.with_layout(Layout::default(), |ui|{
                    egui_extras::TableBuilder::new(ui)
                        .resizable(true)
                        .striped(true)
                        .column(Column::initial(100.))
                        .column(Column::remainder())
                        .header(25., |mut row|{
                            for i in ["Name", "Counter Ui"]{
                                row.col(|ui|{ ui.label(i); });
                            }
                        })
                        .body(|body|{
                            body.rows(
                                65.,
                                self.names.len(),
                                |mut row|{
                                        let index = row.index();
                                        let mut deleted = false;
                                        if index < self.names.len() {
                                            //all index ops should be valid here
                                            row.col(|ui|{
                                                let name = self.names.index_mut(index);
                                                let mut new_name: String = name.as_ref().into();
                                                if ui.text_edit_singleline(&mut new_name).changed() {
                                                    let mut counter = self.counters.remove(&*name).unwrap_or_else(|| default_fn(name.clone()));
                                                    let new_name: Arc<str> = Arc::from(new_name);
                                                    counter.name = new_name.clone();
                                                    self.counters.insert(new_name.clone(), counter);
                                                    *name = new_name;
                                                }
                                                if ui.button("Delete").clicked(){
                                                    deleted = true;
                                                    self.counters.remove(&self.names.remove(index));
                                                }
                                            });
                                            if !deleted {
                                                row.col(|ui|{
                                                    let name = self.names.index(index);
                                                    self.counters.entry(name.clone()).or_insert_with(||default_fn(name.clone())).ui(ui);
                                                });
                                            } else {
                                                ctx.request_repaint();
                                            }
                                    } else {
                                        ctx.request_repaint();
                                        row.col(|ui|{ ui.label("Error: Overread"); });
                                        row.col(|ui|{ ui.label("Error: Overread"); });
                                }
                            }
                        )
                    });
                })
            });
        });
        self.display_popups(ctx, frame)
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        self.save_custom(storage)
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for (_, counter) in self.counters.iter_mut(){
            counter.stop_counter();
        }
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(15)
    }
}