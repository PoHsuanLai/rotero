mod app;
mod db;
mod metadata;
mod state;
mod sync;
mod ui;

fn main() {
    dioxus::launch(app::App);
}
