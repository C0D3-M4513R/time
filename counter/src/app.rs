pub(crate) mod popup;

use std::sync::Arc;
use std::time::Duration;
use eframe::{Frame, Storage};
use egui::{Context, Widget};
use egui::ahash::HashMap;
use egui_extras::Column;
use serde::{Deserialize, Serialize};
use crate::counter_or_timer::CounterTimer;
use crate::get_runtime;

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct App{
    next_name: String,
    names: Vec<Arc<str>>,
    counters: HashMap<Arc<str>, CounterTimer>,
    #[serde(skip)]
    other_app_state: OtherAppState,
}
struct OtherAppState{
    popup: popup::ArcPopupStore
}

impl Default for OtherAppState {
    fn default() -> Self {
        Self {
            popup: Arc::new(Default::default()),
        }
    }
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
                ui.text_edit_singleline(&mut self.next_name);
                if ui.button("Add new Counter").clicked(){
                    let name:Arc<str> = Arc::from(core::mem::take(&mut self.next_name));
                    self.names.push(name.clone());
                    self.counters.insert(name.clone(), default_fn(name));
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
                            if let Some(name) = self.names.get_mut(row.index()){
                                row.col(|ui|{
                                    let mut new_name:String = name.as_ref().into();
                                    if ui.text_edit_singleline(&mut new_name).changed() {
                                        let mut counter = self.counters.remove(&*name).unwrap_or_else(||default_fn(name.clone()));
                                        let new_name:Arc<str> = Arc::from(new_name);
                                        counter.name = new_name.clone();
                                        self.counters.insert(new_name.clone(), counter);
                                        *name = new_name;
                                    }
                                });
                                row.col(|ui|{ self.counters.entry(name.clone()).or_insert_with(||default_fn(name.clone())).ui(ui); });
                            }else{
                                row.col(|ui|{ ui.label("Error: Overread"); });
                                row.col(|ui|{ ui.label("Error: Overread"); });
                            }
                        }
                    )
                })
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