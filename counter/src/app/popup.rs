use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::app::App;

pub(super) type PopupFunc = dyn FnMut(&'_ mut App,&'_ egui::Context, &'_ mut eframe::Frame) -> bool + Send;
pub(super) type PopupStore = VecDeque<Box<PopupFunc>>;
pub(crate) type ArcPopupStore = Arc<Mutex<PopupStore>>;

fn get_id() -> u64 {
    static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

pub fn popup_creator_raw<'a>(
    title: impl Into<egui::WidgetText> + 'a,
    add_content: impl FnMut(&mut App, &mut egui::Ui, u64, &mut bool) + Send + 'static,
) -> Box<PopupFunc> {
    popup_creator_collapsible(title, false, add_content)
}

pub fn popup_creator<'a>(
    popups: ArcPopupStore,
    title: impl Into<egui::WidgetText> + 'a,
    add_content: impl FnMut(&mut App, &mut egui::Ui, u64, &mut bool) + Send + 'static,
) {
    let popup_func = popup_creator_collapsible(title, false, add_content);
    tokio::spawn(async move {
        popups.lock().await.push_back(popup_func)
    });
}

pub fn popup_creator_collapsible<'a>(
    title: impl Into<egui::WidgetText> + 'a,
    collapsible: bool,
    mut add_content: impl FnMut(&mut App, &mut egui::Ui, u64, &mut bool) + 'static + Send,
) -> Box<PopupFunc> {
    let title = title.into();
    let id = get_id();
    let mut open = true;
    Box::new(move |app:&'_ mut App,ctx: &'_ egui::Context, _: &'_ mut eframe::Frame| {
        let mut close_now = false;
        egui::Window::new(title.clone())
            .resizable(false)
            .collapsible(collapsible)
            .open(&mut open)
            .id(egui::Id::new(id))
            .show(ctx, |ui|add_content(app, ui, id, &mut close_now));
        open && !close_now
    })
}

pub fn handle_display_popup<'a, D: std::fmt::Display>(
    popups: &mut PopupStore,
    label: impl Into<egui::WidgetText> + 'a,
    error: &D,
    title: impl Into<egui::WidgetText> + 'a,
) {
    let error_string = error.to_string();
    let label = label.into();
    popups.push_back(popup_creator_raw(title, move |_, ui,_,_| {
        ui.label(label.clone());
        ui.label("Some developer information below:");
        ui.label(&error_string);
    }));
}
pub fn handle_display_popup_arc<'a, D: std::fmt::Display>(
    popups: &ArcPopupStore,
    label: impl Into<egui::WidgetText> + 'a,
    error: &D,
    title: impl Into<egui::WidgetText> + 'a,
) {
    let error_string = error.to_string();
    let label = label.into();
    popup_creator(popups.clone(), title, move |_, ui,_,_| {
        ui.label(label.clone());
        ui.label("Some developer information below:");
        ui.label(&error_string);
    });
}