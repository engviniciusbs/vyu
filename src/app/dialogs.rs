//! Camera, layouts and settings dialogs.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use uuid::Uuid;

use crate::config::{Camera, Config};
use crate::secrets;
use crate::stream::StreamVariant;
use crate::stream::ffmpeg::{FfmpegFactory, probe};
use crate::stream::frame::Frame;
use crate::stream::session::SourceError;

const TAPO_HINT: &str = "Crie a conta da câmera no app Tapo → Configurações da câmera → Avançado → Conta da câmera";
const PROBE_TIMEOUT: Duration = Duration::from_secs(6);

pub enum TestState {
    Idle,
    Running(Receiver<Result<Frame, SourceError>>),
    Ok { texture: egui::TextureHandle, size: (u32, u32) },
    Failed(String),
}

pub struct CameraForm {
    pub editing: Option<Uuid>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub pass: String,
    pub test: TestState,
    pub error: Option<String>,
}

impl Default for CameraForm {
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            host: String::new(),
            port: "554".into(),
            user: String::new(),
            pass: String::new(),
            test: TestState::Idle,
            error: None,
        }
    }
}

pub struct ValidCamera {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
}

impl CameraForm {
    pub fn edit(cam: &Camera) -> Self {
        Self {
            editing: Some(cam.id),
            name: cam.name.clone(),
            host: cam.host.clone(),
            port: cam.port.to_string(),
            user: cam.username.clone(),
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<ValidCamera, String> {
        let name = self.name.trim();
        let host = self.host.trim();
        let user = self.user.trim();
        if name.is_empty() {
            return Err("Dê um nome à câmera".into());
        }
        if host.is_empty() {
            return Err("Informe o IP da câmera".into());
        }
        if user.is_empty() {
            return Err("Informe o usuário da conta da câmera".into());
        }
        let port: u16 = self.port.trim().parse().ok().filter(|p| *p > 0).ok_or("Porta inválida (use 554)")?;
        if self.editing.is_none() && self.pass.is_empty() {
            return Err("Informe a senha da conta da câmera".into());
        }
        Ok(ValidCamera { name: name.into(), host: host.into(), port, user: user.into() })
    }

    /// The password to use: the typed one, or the stored one when editing.
    fn password(&self) -> Option<String> {
        if !self.pass.is_empty() {
            return Some(self.pass.clone());
        }
        self.editing.and_then(|id| secrets::get_password(id).ok().flatten())
    }

    fn start_test(&mut self, ctx: &egui::Context) {
        let v = match self.validate() {
            Ok(v) => v,
            Err(e) => {
                self.test = TestState::Failed(e);
                return;
            }
        };
        let Some(pass) = self.password() else {
            self.test = TestState::Failed("Informe a senha".into());
            return;
        };
        let factory = FfmpegFactory { host: v.host, port: v.port, user: v.user, pass };
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(probe(&factory, StreamVariant::Low, PROBE_TIMEOUT));
            ctx.request_repaint();
        });
        self.test = TestState::Running(rx);
    }

    fn poll_test(&mut self, ctx: &egui::Context) {
        let TestState::Running(rx) = &self.test else { return };
        match rx.try_recv() {
            Ok(Ok(frame)) => {
                let img = egui::ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &frame.rgba);
                let texture = ctx.load_texture("vyu-probe", img, egui::TextureOptions::LINEAR);
                self.test = TestState::Ok { texture, size: (frame.width, frame.height) };
            }
            Ok(Err(e)) => self.test = TestState::Failed(e.message()),
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
            Err(mpsc::TryRecvError::Disconnected) => self.test = TestState::Failed("O teste foi interrompido".into()),
        }
    }

    /// Writes the form into the config and the Credential Manager.
    fn save(&self, cfg: &mut Config) -> Result<(Uuid, bool), String> {
        let v = self.validate()?;
        let (id, is_new) = match self.editing {
            Some(id) => {
                let cam = cfg.cameras.iter_mut().find(|c| c.id == id).ok_or("Câmera não encontrada")?;
                cam.name = v.name;
                cam.host = v.host;
                cam.port = v.port;
                cam.username = v.user;
                (id, false)
            }
            None => {
                let mut cam = Camera::new(&v.name, &v.host, &v.user);
                cam.port = v.port;
                let id = cam.id;
                cfg.cameras.push(cam);
                (id, true)
            }
        };
        if !self.pass.is_empty() {
            if let Err(e) = secrets::set_password(id, &self.pass) {
                if is_new {
                    cfg.cameras.retain(|c| c.id != id);
                }
                return Err(e.to_string());
            }
        }
        Ok((id, is_new))
    }
}

#[derive(Default)]
pub struct LayoutsForm {
    new_name: String,
    renaming: Option<(String, String)>,
    error: Option<String>,
}

pub enum Dialog {
    None,
    Camera(CameraForm),
    Layouts(LayoutsForm),
    Settings,
}

impl Dialog {
    pub fn is_open(&self) -> bool {
        !matches!(self, Dialog::None)
    }

    pub fn layouts() -> Self {
        Dialog::Layouts(LayoutsForm::default())
    }
}

/// What the app must do after the dialog ran this frame.
#[derive(Default)]
pub struct DialogOutcome {
    pub changed: bool,
    /// Camera created (true) or edited (false): restart its session.
    pub saved_camera: Option<(Uuid, bool)>,
    /// Show (true) or hide (false) a camera in the active layout.
    pub toggle_tile: Option<(Uuid, bool)>,
    pub settings_changed: bool,
}

pub fn show(ctx: &egui::Context, dialog: &mut Dialog, cfg: &mut Config) -> DialogOutcome {
    let mut out = DialogOutcome::default();
    let mut close = false;
    match dialog {
        Dialog::None => return out,
        Dialog::Camera(form) => {
            form.poll_test(ctx);
            let title = if form.editing.is_some() { "Editar câmera" } else { "Adicionar câmera" };
            let resp = egui::Modal::new(egui::Id::unique("vyu-camera-dialog")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(title);
                ui.add_space(8.0);
                egui::Grid::new("cam-form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Nome");
                    ui.text_edit_singleline(&mut form.name).on_hover_text("Ex.: Sala, Garagem");
                    ui.end_row();
                    ui.label("IP");
                    ui.text_edit_singleline(&mut form.host).on_hover_text("Ex.: 192.168.0.20 (veja no app Tapo)");
                    ui.end_row();
                    ui.label("Porta");
                    ui.add(egui::TextEdit::singleline(&mut form.port).desired_width(60.0));
                    ui.end_row();
                    ui.label("Usuário");
                    ui.text_edit_singleline(&mut form.user);
                    ui.end_row();
                    ui.label("Senha");
                    let hint = if form.editing.is_some() { "deixe vazio para manter" } else { "" };
                    ui.add(egui::TextEdit::singleline(&mut form.pass).password(true).hint_text(hint));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new(TAPO_HINT).small().color(Color32::from_gray(150)));
                ui.add_space(8.0);

                match &form.test {
                    TestState::Idle => {}
                    TestState::Running(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Testando conexão…");
                        });
                    }
                    TestState::Ok { texture, size } => {
                        let w = ui.available_width();
                        let h = w * size.1 as f32 / size.0.max(1) as f32;
                        ui.image((texture.id(), egui::vec2(w, h)));
                        ui.colored_label(Color32::from_rgb(74, 222, 128), "✔ Conectou!");
                    }
                    TestState::Failed(msg) => {
                        ui.colored_label(Color32::from_rgb(248, 113, 113), format!("✖ {msg}"));
                    }
                }
                if let Some(e) = &form.error {
                    ui.colored_label(Color32::from_rgb(248, 113, 113), e);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let testing = matches!(form.test, TestState::Running(_));
                    if ui.add_enabled(!testing, egui::Button::new("Testar conexão")).clicked() {
                        form.start_test(ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Salvar").strong()).clicked() {
                            match form.save(cfg) {
                                Ok(saved) => {
                                    out.saved_camera = Some(saved);
                                    out.changed = true;
                                    close = true;
                                }
                                Err(e) => form.error = Some(e),
                            }
                        }
                        if ui.button("Cancelar").clicked() {
                            close = true;
                        }
                    });
                });
            });
            close |= resp.should_close();
        }
        Dialog::Layouts(form) => {
            let resp = egui::Modal::new(egui::Id::unique("vyu-layouts-dialog")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading("Layouts");
                ui.add_space(8.0);
                let names: Vec<String> = cfg.layouts.iter().map(|l| l.name.clone()).collect();
                let can_delete = names.len() > 1;
                for (i, name) in names.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if let Some((old, new)) = &mut form.renaming {
                            if old == name {
                                ui.text_edit_singleline(new);
                                if ui.button("OK").clicked() {
                                    match cfg.rename_layout(old, new) {
                                        Ok(()) => {
                                            form.renaming = None;
                                            form.error = None;
                                            out.changed = true;
                                        }
                                        Err(e) => form.error = Some(e),
                                    }
                                }
                                return;
                            }
                        }
                        let active = cfg.active_layout == *name;
                        let label = if i < 9 { format!("{name}   (Ctrl+{})", i + 1) } else { name.clone() };
                        if ui.selectable_label(active, label).clicked() {
                            cfg.active_layout = name.clone();
                            out.changed = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(can_delete, egui::Button::new("🗑")).on_hover_text("Apagar").clicked() {
                                cfg.delete_layout(name);
                                out.changed = true;
                            }
                            if ui.button("✏").on_hover_text("Renomear").clicked() {
                                form.renaming = Some((name.clone(), name.clone()));
                            }
                        });
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.new_name).hint_text("Nome do novo layout").desired_width(200.0));
                    if ui.button("Salvar atual como…").clicked() {
                        match cfg.save_layout_as(&form.new_name) {
                            Ok(()) => {
                                form.new_name.clear();
                                form.error = None;
                                out.changed = true;
                            }
                            Err(e) => form.error = Some(e),
                        }
                    }
                });
                if let Some(e) = &form.error {
                    ui.colored_label(Color32::from_rgb(248, 113, 113), e);
                }
                if !cfg.cameras.is_empty() {
                    ui.separator();
                    ui.label(format!("Câmeras em \"{}\"", cfg.active().name));
                    let shown: Vec<Uuid> = cfg.active().tiles.iter().map(|t| t.camera_id).collect();
                    for cam in &cfg.cameras {
                        let mut on = shown.contains(&cam.id);
                        if ui.checkbox(&mut on, &cam.name).changed() {
                            out.toggle_tile = Some((cam.id, on));
                        }
                    }
                }
                ui.add_space(6.0);
                if ui.button("Fechar").clicked() {
                    close = true;
                }
            });
            close |= resp.should_close();
        }
        Dialog::Settings => {
            let resp = egui::Modal::new(egui::Id::unique("vyu-settings-dialog")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading("Configurações");
                ui.add_space(8.0);
                if ui.checkbox(&mut cfg.settings.start_with_windows, crate::autostart::LABEL).changed() {
                    out.settings_changed = true;
                    out.changed = true;
                }
                if ui.checkbox(&mut cfg.settings.start_fullscreen, "Abrir em tela cheia").changed() {
                    out.changed = true;
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Atalhos: F11 tela cheia · Ctrl+N nova câmera · Ctrl+1…9 layouts · M mudo · Esc sair").small().color(Color32::from_gray(150)));
                ui.add_space(6.0);
                if ui.button("Fechar").clicked() {
                    close = true;
                }
            });
            close |= resp.should_close();
        }
    }
    if close {
        *dialog = Dialog::None;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> CameraForm {
        CameraForm {
            name: "Sala".into(),
            host: " 192.168.0.20 ".into(),
            port: "554".into(),
            user: "cam".into(),
            pass: "x".into(),
            ..CameraForm::default()
        }
    }

    #[test]
    fn valid_form_is_trimmed() {
        let v = form().validate().unwrap();
        assert_eq!(v.host, "192.168.0.20");
        assert_eq!(v.port, 554);
    }

    #[test]
    fn requires_name_host_user() {
        for clear in [|f: &mut CameraForm| f.name.clear(), |f: &mut CameraForm| f.host.clear(), |f: &mut CameraForm| f.user.clear()] {
            let mut f = form();
            clear(&mut f);
            assert!(f.validate().is_err());
        }
    }

    #[test]
    fn port_must_be_valid() {
        for bad in ["0", "70000", "abc", ""] {
            let mut f = form();
            f.port = bad.into();
            assert!(f.validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn new_camera_requires_password_but_edit_does_not() {
        let mut f = form();
        f.pass.clear();
        assert!(f.validate().is_err());
        f.editing = Some(uuid::Uuid::new_v4());
        assert!(f.validate().is_ok());
    }
}
