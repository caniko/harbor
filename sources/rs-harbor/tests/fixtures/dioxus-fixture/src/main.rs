use dioxus::prelude::*;

fn App() -> Element {
    rsx! {
        main { "rs-harbor Dioxus fixture" }
    }
}

fn main() {
    dioxus::launch(App);
}
