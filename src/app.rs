use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use serde_json::{from_str, to_string, Value};
use std::collections::VecDeque;
extern crate jsonpath_lib as jsonpath;
/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    // this how you opt-out of serialization of a member
    #[serde(skip)]
    value: f32,
    url: String,
    error: String,
    #[serde(skip)]
    frontend: Option<FrontEnd>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            error: "".to_string(),
            url: "wss://ws-feed.exchange.coinbase.com".to_string(),
            frontend: None,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customized the look at feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
        //

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        let mut s = TemplateApp::default();
        s.connect(cc.egui_ctx.clone());
        s
    }
    fn connect(&mut self, ctx: egui::Context) {
        let wakeup = move || ctx.request_repaint(); // wake up UI thread on new message
        match ewebsock::connect_with_wakeup(&self.url, wakeup) {
            Ok((ws_sender, ws_receiver)) => {
                self.frontend = Some(FrontEnd::new(ws_sender, ws_receiver));
                self.error.clear();
            }
            Err(error) => {
                println!("Failed to connect to {:?}: {}", &self.url, error);
                self.error = error;
            }
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Examples of how to create different panels and windows.
        // Pick whichever suits you.
        // Tip: a good default choice is to just keep the `CentralPanel`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("server").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("URL:");
                if ui.text_edit_singleline(&mut self.url).lost_focus()
                    && ui.input().key_pressed(egui::Key::Enter)
                {
                    self.connect(ctx.clone());
                }
            });
        });

        if !self.error.is_empty() {
            egui::TopBottomPanel::top("error").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Error:");
                    ui.colored_label(egui::Color32::RED, &self.error);
                });
            });
        }

        if let Some(frontend) = &mut self.frontend {
            frontend.ui(ctx);
        }
    }
}

struct FrontEnd {
    ws_sender: WsSender,
    ws_receiver: WsReceiver,
    events: VecDeque<WsEvent>,
    text_to_send: String,
    json_path_filter: String,
}

impl FrontEnd {
    fn new(ws_sender: WsSender, ws_receiver: WsReceiver) -> Self {
        Self {
            ws_sender,
            ws_receiver,
            events: Default::default(),
            text_to_send: "{\"type\": \"subscribe\",\"product_ids\": [\"BTC-USD\"],\"channels\": [\"level2\"]}".to_string(),
            json_path_filter: String::from("$"),
        }
    }

    fn ui(&mut self, ctx: &egui::Context) {
        while let Some(event) = self.ws_receiver.try_recv() {
            self.events.push_back(event);
            if self.events.len() > 99 {
                self.events.pop_front();
            }
            ctx.request_repaint()
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Message to send:");
                if ui.text_edit_singleline(&mut self.text_to_send).lost_focus()
                    && ui.input().key_pressed(egui::Key::Enter)
                {
                    self.ws_sender
                        .send(WsMessage::Text(std::mem::take(&mut self.text_to_send)));
                }

                ui.label("filter:");
                if ui
                    .text_edit_singleline(&mut self.json_path_filter)
                    .lost_focus()
                    && ui.input().key_pressed(egui::Key::Enter)
                {
                    self.json_path_filter = std::mem::take(&mut self.json_path_filter);
                }
            });

            ui.separator();
            ui.heading("Received events:");
            for event in &self.events {
                if let WsEvent::Message(msg) = event {
                    if let WsMessage::Text(txt) = msg {
                        let json = from_str::<Value>(&txt).unwrap();
                        let mut selector = jsonpath::selector(&json);
                        // let path = "$.path";
                        let result = match selector(&self.json_path_filter) {
                            Ok(value) => {
                                let joined = value
                                    .into_iter()
                                    .map(|v| match to_string(&v) {
                                        Ok(str) => str,
                                        Err(err) => err.to_string(),
                                    })
                                    .collect::<Vec<String>>();
                                joined.join(" ,")
                            }

                            Err(err) => err.to_string(),
                        };
                        //
                        ui.label(format!("{}", result));
                    } else {
                        ui.label(format!("non_text {:?}", msg));
                    }
                } else {
                    ui.label(format!("non_msg {:?}", event));
                }
            }
        });
    }
}
