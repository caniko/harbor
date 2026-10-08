use dioxus::prelude::*;

fn App() -> Element {
    rsx! {
        main { "harbor-rs Dioxus fixture" }
    }
}

fn main() {
    dioxus::launch(App);
}
