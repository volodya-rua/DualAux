use crate::{sys, Dev};
use eframe::egui::{self, vec2, Color32, Id, RichText, Rounding, Sense, Stroke};
use std::time::{Duration, Instant};

const MAX_OUTPUTS: usize = 5;
const DELAY_MAX: u32 = 500; // keep equal to sys::MAX_DELAY_MS on Windows
const BG: Color32 = Color32::from_rgb(5, 5, 5);
const CARD: Color32 = Color32::from_rgb(13, 13, 13);
const LINE: Color32 = Color32::from_rgb(28, 28, 28);
const DIM: Color32 = Color32::from_rgb(120, 120, 120);
const ON: Color32 = Color32::from_rgb(74, 222, 128);

struct Out {
    id: String,
    vol: f32,
    delay: u32, // ms
}

struct App {
    sys: sys::Sys,
    devs: Vec<Dev>,
    fixed_a: Option<String>,
    outs: Vec<Out>,
    on: bool,
    msg: String,
    last: Instant,
    tray: bool,
    #[cfg(windows)]
    _tray: Option<tray_icon::TrayIcon>,
}

/// Two overlapping rings on a rounded black tile; used for the window and tray icon.
fn icon_rgba(n: u32) -> Vec<u8> {
    let s = n as f32;
    let mut v = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5 - s / 2.0, y as f32 + 0.5 - s / 2.0);
            let q = (px.abs() - (s / 2.0 - 0.22 * s), py.abs() - (s / 2.0 - 0.22 * s));
            let d = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt() + q.0.max(q.1).min(0.0) - 0.22 * s;
            let tile = (0.5 - d).clamp(0.0, 1.0);
            let ring = |cx: f32| {
                let d = ((px - cx * s).powi(2) + py.powi(2)).sqrt();
                (0.5 * 0.085 * s - (d - 0.2 * s).abs() + 0.5).clamp(0.0, 1.0)
            };
            let r = ring(-0.12).max(ring(0.12));
            let c = (12.0 + 243.0 * r) as u8;
            v.extend_from_slice(&[c, c, c, (tile * 255.0) as u8]);
        }
    }
    v
}

/// Windows notification-area icon: closing the window hides it here instead of quitting.
#[cfg(windows)]
fn make_tray(ctx: &egui::Context) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    fn show(c: &egui::Context) {
        c.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        c.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        c.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    let menu = Menu::new();
    let open = MenuItem::new("Open DualAux", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;
    let (oid, qid) = (open.id().clone(), quit.id().clone());
    let c = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == qid {
            std::process::exit(0);
        }
        if e.id == oid {
            show(&c);
        }
    }));
    let c = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            show(&c);
        }
    }));
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("DualAux")
        .with_icon(Icon::from_rgba(icon_rgba(32), 32, 32).ok()?)
        .build()
        .ok()
}

fn slider(ui: &mut egui::Ui, v: &mut f32, w: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click_and_drag());
    let (x0, x1, y) = (rect.left() + 8.0, rect.right() - 8.0, rect.center().y);
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let nv = ((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            changed = (nv - *v).abs() > f32::EPSILON;
            *v = nv;
        }
    }
    let hot = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || resp.dragged(), 0.15);
    let hx = x0 + (x1 - x0) * *v;
    let p = ui.painter();
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, y - 2.0), egui::pos2(x1, y + 2.0)), Rounding::same(2.0), Color32::from_rgb(34, 34, 34));
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, y - 2.0), egui::pos2(hx, y + 2.0)), Rounding::same(2.0), Color32::from_rgb(235, 235, 235));
    p.circle_filled(egui::pos2(hx, y), 5.5 + 1.5 * hot, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    changed
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_theme(egui::Theme::Dark);
        #[cfg(windows)]
        if let Ok(b) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
            let mut f = egui::FontDefinitions::default();
            f.font_data.insert("segoe".into(), egui::FontData::from_owned(b));
            f.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "segoe".into());
            ctx.set_fonts(f);
        }
        ctx.style_mut(|s| {
            let v = &mut s.visuals;
            v.panel_fill = BG;
            v.window_fill = Color32::from_rgb(10, 10, 10);
            v.window_stroke = Stroke::new(1.0, LINE);
            v.menu_rounding = Rounding::same(14.0);
            v.popup_shadow = egui::epaint::Shadow { offset: vec2(0.0, 8.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(160) };
            v.extreme_bg_color = Color32::BLACK;
            v.selection.bg_fill = Color32::from_rgb(38, 38, 38);
            v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.rounding = Rounding::same(12.0);
                w.weak_bg_fill = Color32::BLACK;
                w.bg_stroke = Stroke::new(1.0, LINE);
            }
            v.widgets.hovered.weak_bg_fill = Color32::from_rgb(18, 18, 18);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(52, 52, 52));
            v.widgets.active.weak_bg_fill = Color32::from_rgb(24, 24, 24);
            s.spacing.interact_size.y = 38.0;
            s.spacing.button_padding = vec2(14.0, 8.0);
            s.spacing.scroll = egui::style::ScrollStyle::floating();
            s.animation_time = 0.18;
        });
        #[cfg(windows)]
        let tray_icon = make_tray(ctx);
        let sys = sys::Sys::new();
        let mut app = App {
            fixed_a: sys.fixed_a(), devs: vec![], sys, outs: vec![],
            on: false, msg: String::new(), last: Instant::now(),
            #[cfg(windows)]
            tray: tray_icon.is_some(),
            #[cfg(not(windows))]
            tray: false,
            #[cfg(windows)]
            _tray: tray_icon,
        };
        app.refresh();
        app
    }

    /// Cards the user must fill: Windows' own default already counts as one output.
    fn min_outs(&self) -> usize {
        if self.fixed_a.is_some() { 1 } else { 2 }
    }

    fn unused(&self) -> Option<String> {
        self.devs.iter().find(|d| !self.outs.iter().any(|o| o.id == d.id)).map(|d| d.id.clone())
    }

    fn refresh(&mut self) {
        self.devs = self.sys.devices();
        self.fixed_a = self.sys.fixed_a();
        let ids: Vec<_> = self.devs.iter().map(|d| d.id.clone()).collect();
        self.outs.retain(|o| ids.contains(&o.id));
        while self.outs.len() < self.min_outs() {
            match self.unused() {
                Some(id) => self.outs.push(Out { id, vol: 1.0, delay: 0 }),
                None => break,
            }
        }
        self.last = Instant::now();
    }

    fn label(&self, id: &str) -> String {
        self.devs.iter().find(|d| d.id == id).map(|d| d.label.clone()).unwrap_or_else(|| "None".into())
    }

    fn frame(ctx: &egui::Context, on: bool) -> egui::Frame {
        let t = ctx.animate_bool_with_time(Id::new("live"), on, 0.5);
        egui::Frame::none()
            .fill(CARD)
            .stroke(Stroke::new(1.0, LINE.lerp_to_gamma(Color32::from_rgb(30, 64, 44), t)))
            .rounding(Rounding::same(20.0))
            .inner_margin(egui::Margin::symmetric(18.0, 16.0))
    }

    fn small(text: &str) -> RichText {
        RichText::new(text.to_uppercase()).size(10.5).color(DIM)
    }

    /// Returns true if the remove button was clicked.
    fn card(&mut self, ui: &mut egui::Ui, i: usize, title: String) -> bool {
        let mut remove = false;
        let can_remove = !self.on && self.outs.len() > self.min_outs();
        let taken: Vec<String> = self.outs.iter().map(|o| o.id.clone()).collect();
        let mut vol_changed = false;
        let mut delay_changed = false;
        Self::frame(ui.ctx(), self.on).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(Self::small(&title));
                if can_remove {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = ui.add(egui::Button::new(RichText::new("×").size(17.0).color(DIM)).frame(false));
                        remove = b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
                    });
                }
            });
            ui.add_space(6.0);
            let cur = self.outs[i].id.clone();
            let mut pick = cur.clone();
            ui.add_enabled_ui(!self.on, |ui| {
                egui::ComboBox::from_id_salt(i)
                    .width(ui.available_width())
                    .selected_text(self.label(&cur))
                    .show_ui(ui, |ui| {
                        for d in self.devs.iter().filter(|d| d.id == cur || !taken.contains(&d.id)) {
                            ui.selectable_value(&mut pick, d.id.clone(), &d.label);
                        }
                    });
            });
            self.outs[i].id = pick;
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let w = ui.available_width() - 44.0;
                vol_changed = slider(ui, &mut self.outs[i].vol, w);
                ui.label(RichText::new(format!("{}%", (self.outs[i].vol * 100.0).round() as i32)).size(12.0).color(DIM));
            });
            // Sync delay holds this output back so Bluetooth devices of different latency line up.
            // Windows only; the other platforms have no per-output control.
            if cfg!(windows) {
                ui.add_space(4.0);
                ui.label(Self::small("Sync delay"));
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 52.0;
                    let mut v = self.outs[i].delay as f32 / DELAY_MAX as f32;
                    if slider(ui, &mut v, w) {
                        self.outs[i].delay = ((v * DELAY_MAX as f32 / 10.0).round() as u32) * 10; // 10 ms steps
                        delay_changed = true;
                    }
                    ui.label(RichText::new(format!("{} ms", self.outs[i].delay)).size(12.0).color(DIM));
                });
            }
        });
        if vol_changed && self.on {
            self.sys.vol(i, &self.outs[i].id, self.outs[i].vol);
        }
        if delay_changed && self.on {
            self.sys.delay(i, self.outs[i].delay);
        }
        remove
    }

    fn header(&self, ui: &mut egui::Ui) {
        let t = ui.ctx().animate_bool_with_time(Id::new("live"), self.on, 0.5);
        ui.horizontal(|ui| {
            ui.label(RichText::new("DualAux").size(22.0).strong().color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(if self.on { "Live" } else { "Idle" }).size(12.0).color(DIM.lerp_to_gamma(ON, t)));
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                let pulse = 0.5 + 0.5 * (ui.input(|i| i.time) as f32 * 2.5).sin();
                let c = r.center();
                ui.painter().circle_filled(c, 3.0 + 4.0 * pulse * t, Color32::from_rgba_unmultiplied(74, 222, 128, (50.0 * t) as u8));
                ui.painter().circle_filled(c, 3.5, Color32::from_rgb(52, 52, 52).lerp_to_gamma(ON, t));
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(if self.on { Duration::from_millis(33) } else { Duration::from_secs(2) });
        if ctx.input(|i| i.viewport().close_requested()) && self.tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if !self.on && self.last.elapsed() > Duration::from_secs(2) {
            self.refresh();
        }
        egui::TopBottomPanel::bottom("go")
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 24.0, right: 24.0, top: 4.0, bottom: 24.0 }))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| ui.label(RichText::new(&self.msg).size(12.0).color(DIM)));
                ui.add_space(8.0);
                let t = ctx.animate_bool_with_time(Id::new("btn"), self.on, 0.3);
                let fill = Color32::WHITE.lerp_to_gamma(BG, t);
                let fg = Color32::BLACK.lerp_to_gamma(ON, t);
                let b = egui::Button::new(RichText::new(if self.on { "Stop" } else { "Start" }).size(15.0).strong().color(fg))
                    .fill(fill)
                    .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(74, 222, 128, (t * 255.0) as u8)))
                    .rounding(Rounding::same(26.0))
                    ;
                if ui.add_sized([ui.available_width(), 52.0], b).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    if self.on {
                        self.sys.stop();
                        self.on = false;
                        self.msg.clear();
                    } else if self.outs.len() < self.min_outs() {
                        self.msg = "Connect another output device first".into();
                    } else {
                        let ids: Vec<_> = self.outs.iter().map(|o| o.id.clone()).collect();
                        let vols: Vec<_> = self.outs.iter().map(|o| o.vol).collect();
                        let delays: Vec<_> = self.outs.iter().map(|o| o.delay).collect();
                        match self.sys.start(&ids, &vols, &delays) {
                            Ok(()) => {
                                self.on = true;
                                self.msg = format!("Playing on {} outputs", ids.len() + self.fixed_a.is_some() as usize)
                            }
                            Err(e) => self.msg = e,
                        }
                    }
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 24.0, right: 24.0, top: 24.0, bottom: 0.0 }))
            .show(ctx, |ui| {
                self.header(ui);
                ui.add_space(18.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let off = self.fixed_a.is_some() as usize;
                    if let Some(name) = self.fixed_a.clone() {
                        Self::frame(ctx, self.on).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(Self::small("Output 1"));
                            ui.add_space(4.0);
                            ui.label(RichText::new(name).size(14.0).color(Color32::WHITE));
                            ui.label(RichText::new("Windows default · plays as usual").size(12.0).color(DIM));
                        });
                        ui.add_space(12.0);
                    }
                    let mut gone = None;
                    for i in 0..self.outs.len() {
                        if self.card(ui, i, format!("Output {}", i + 1 + off)) {
                            gone = Some(i);
                        }
                        ui.add_space(12.0);
                    }
                    if let Some(i) = gone {
                        self.outs.remove(i);
                    }
                    if !self.on && self.outs.len() + off < MAX_OUTPUTS {
                        if let Some(id) = self.unused() {
                            let add = egui::Button::new(RichText::new("+   Add device").size(13.0).color(DIM))
                                .rounding(Rounding::same(20.0));
                            if ui.add_sized([ui.available_width(), 44.0], add).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.outs.push(Out { id, vol: 1.0, delay: 0 });
                            }
                            ui.add_space(12.0);
                        }
                    }
                });
            });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.sys.stop();
    }
}

pub fn run() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 640.0])
            .with_min_inner_size([380.0, 440.0])
            .with_icon(egui::IconData { rgba: icon_rgba(64), width: 64, height: 64 }),
        ..Default::default()
    };
    eframe::run_native("DualAux", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
