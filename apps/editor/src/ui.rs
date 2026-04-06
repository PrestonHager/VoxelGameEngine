//! egui layout shared between eframe and embedded runners.

use crate::launcher;
use crate::model::{EditorMainTab, EditorModel};
use eframe::egui;
use eframe::egui::{menu, Button, Color32, Key, KeyboardShortcut, Modifiers, Sense, Stroke};
use engine_core::EngineState;
use scene::{ids, AssetKind, PrefabCategory, PrefabLibrary, TerrainMode};
use tracing::debug;

fn kb_open() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::O)
}
fn kb_save() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::S)
}
fn kb_save_as() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)
}

fn handle_menu_shortcuts(ctx: &egui::Context, model: &mut EditorModel) {
    let open = kb_open();
    let save = kb_save();
    let save_as = kb_save_as();
    if ctx.input_mut(|i| i.consume_shortcut(&open)) {
        model.open_level_dialog();
    }
    if ctx.input_mut(|i| i.consume_shortcut(&save_as)) {
        model.save_level_as_dialog();
    } else if ctx.input_mut(|i| i.consume_shortcut(&save)) {
        if let Err(e) = model.save_level_file() {
            model.status.clone_from(&e);
            model.push_log(e);
        } else {
            model.status.clear();
        }
    }
}

pub fn draw_editor_ui(
    ctx: &egui::Context,
    model: &mut EditorModel,
    embedded: Option<&mut EngineState>,
) {
    if embedded.is_none() {
        model.engine_viewport_px = None;
        model.bootstrap_external();
    } else {
        model.bootstrap_embedded();
    }

    handle_menu_shortcuts(ctx, model);

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Project…").clicked() {
                    model.new_project_dialog();
                    ui.close_menu();
                }
                if ui.button("Open Project…").clicked() {
                    model.open_project_dialog();
                    ui.close_menu();
                }
                if ui.button("Save Project").clicked() {
                    if let Err(e) = model.save_project_file() {
                        model.status.clone_from(&e);
                        model.push_log(e);
                    } else {
                        model.status.clear();
                    }
                    ui.close_menu();
                }
                if ui.button("Save Project As…").clicked() {
                    model.save_project_as_dialog();
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .add(Button::new("Open…").shortcut_text(ctx.format_shortcut(&kb_open())))
                    .clicked()
                {
                    model.open_level_dialog();
                    ui.close_menu();
                }
                if ui
                    .add(Button::new("Save").shortcut_text(ctx.format_shortcut(&kb_save())))
                    .clicked()
                {
                    if let Err(e) = model.save_level_file() {
                        model.status.clone_from(&e);
                        model.push_log(e);
                    } else {
                        model.status.clear();
                    }
                    ui.close_menu();
                }
                if ui
                    .add(Button::new("Save As…").shortcut_text(ctx.format_shortcut(&kb_save_as())))
                    .clicked()
                {
                    model.save_level_as_dialog();
                    ui.close_menu();
                }
            });
            ui.menu_button("Edit", |ui| {
                ui.add_enabled(false, Button::new("Undo"));
                ui.add_enabled(false, Button::new("Redo"));
                ui.separator();
                ui.label(
                    egui::RichText::new("Undo / redo — coming later")
                        .small()
                        .weak(),
                );
            });
        });
    });

    egui::TopBottomPanel::top("main_tabs").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut model.main_tab, EditorMainTab::Level, "Level");
            ui.selectable_value(&mut model.main_tab, EditorMainTab::Assets, "Assets");
        });
    });

    if embedded.is_some() {
        egui::TopBottomPanel::bottom("embedded_log")
            .resizable(true)
            .default_height(140.0)
            .min_height(72.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Log").small().weak());
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for line in &model.log {
                            ui.monospace(line);
                        }
                    });
            });
    }

    egui::SidePanel::left("prefabs")
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Library");
            ui.label(egui::RichText::new("Built-in prefabs (ECS scene objects)").weak());
            ui.separator();
            for cat in [
                PrefabCategory::Primitive,
                PrefabCategory::Gameplay,
                PrefabCategory::Environment,
                PrefabCategory::Utility,
            ] {
                ui.collapsing(format!("{cat:?}"), |ui| {
                    for p in PrefabLibrary::builtin()
                        .iter()
                        .filter(|p| p.category == cat)
                    {
                        if ui.button(&p.name).clicked() {
                            model.add_placed(p.id, &p.name);
                        }
                    }
                });
            }
        });

    egui::SidePanel::right("scene")
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.heading("Scene");
            ui.label("Placed objects");
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for o in &model.level.objects {
                        let sel = model.selected_instance == Some(o.instance_id);
                        let label = format!("{} (#{})", o.name, o.instance_id);
                        if ui.selectable_label(sel, label).clicked() {
                            model.selected_instance = Some(o.instance_id);
                        }
                    }
                });
            if ui.button("Delete selected").clicked() {
                if let Some(id) = model.selected_instance {
                    model.level.objects.retain(|o| o.instance_id != id);
                    model.selected_instance = None;
                    model.push_log(format!("Removed instance {id}."));
                }
            }

            if let Some(id) = model.selected_instance {
                if let Some(o) = model.level.objects.iter_mut().find(|o| o.instance_id == id) {
                    ui.separator();
                    ui.label(format!("Edit #{}", id));
                    ui.horizontal(|ui| {
                        ui.label("name");
                        ui.text_edit_singleline(&mut o.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut o.position[0]).speed(0.1));
                        ui.label("y");
                        ui.add(egui::DragValue::new(&mut o.position[1]).speed(0.1));
                        ui.label("z");
                        ui.add(egui::DragValue::new(&mut o.position[2]).speed(0.1));
                    });
                    ui.checkbox(&mut o.visible, "visible");

                    ui.separator();
                    ui.label("Script (Lua asset)");
                    let scripts: Vec<(String, String)> = model
                        .level
                        .assets
                        .iter()
                        .filter(|a| a.kind == AssetKind::Script)
                        .map(|a| (a.id.clone(), a.name.clone()))
                        .collect();
                    let selected_label = o
                        .script_asset_id
                        .as_deref()
                        .and_then(|sid| model.level.assets.iter().find(|a| a.id == sid))
                        .filter(|a| a.kind == AssetKind::Script)
                        .map(|a| a.name.as_str())
                        .unwrap_or("None");
                    egui::ComboBox::from_id_salt("obj_script_asset")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(o.script_asset_id.is_none(), "None")
                                .clicked()
                            {
                                o.script_asset_id = None;
                            }
                            for (id, name) in &scripts {
                                let chosen = o.script_asset_id.as_deref() == Some(id.as_str());
                                if ui.selectable_label(chosen, name).clicked() {
                                    o.script_asset_id = Some(id.clone());
                                }
                            }
                        });

                    if o.prefab_id == ids::CAMERA {
                        let cam = o.camera.get_or_insert_with(Default::default);
                        ui.separator();
                        ui.label("Camera rig");
                        ui.add(
                            egui::DragValue::new(&mut cam.fov_deg)
                                .speed(1.0)
                                .prefix("fov ° "),
                        );
                        ui.horizontal(|ui| {
                            ui.label("yaw °");
                            ui.add(egui::DragValue::new(&mut cam.yaw_deg).speed(0.5));
                            ui.label("pitch °");
                            ui.add(egui::DragValue::new(&mut cam.pitch_deg).speed(0.5));
                        });
                        ui.checkbox(&mut cam.active, "active (first active wins)");
                    }
                }
            }

            ui.separator();
            ui.collapsing("Terrain (MVP)", |ui| {
                ui.label(format!("Mode: {:?}", TerrainMode::Flat));
                ui.horizontal(|ui| {
                    ui.label("surface material");
                    ui.add(
                        egui::DragValue::new(&mut model.level.terrain.surface_material).speed(1.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("base height (voxels)");
                    ui.add(
                        egui::DragValue::new(&mut model.level.terrain.base_height_voxels)
                            .speed(1.0),
                    );
                });
            });
        });

    egui::CentralPanel::default().show(ctx, |ui| match model.main_tab {
        EditorMainTab::Level => {
            draw_level_tab(ui, model, embedded);
        }
        EditorMainTab::Assets => {
            if embedded.is_some() {
                if model.engine_viewport_px.is_some() {
                    debug!(
                        target: "vge_embedded",
                        "clearing engine_viewport_px (Assets tab hides embedded 3D view)"
                    );
                }
                model.engine_viewport_px = None;
            }
            draw_assets_tab(ui, model);
        }
    });
}

fn draw_level_tab(ui: &mut egui::Ui, model: &mut EditorModel, embedded: Option<&mut EngineState>) {
    let is_embedded = embedded.is_some();
    ui.heading("Voxel Editor");
    if !is_embedded {
        ui.label(format!(
            "IPC port: {} (set VGE_IPC_PORT to change)",
            model.port
        ));
        ui.label(egui::RichText::new(
            "External engine: Play pushes the saved level to engine-runner over IPC. Use File for Open / Save.",
        )
        .weak());
    } else {
        ui.label(egui::RichText::new(
            "Frameless 3D view is embedded in the center (child window). File ▶ Open/Save. ▶ Play applies in-process.",
        )
        .weak());
    }
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Level name");
        ui.text_edit_singleline(&mut model.level.name);
    });
    ui.horizontal(|ui| {
        ui.label("Level file");
        ui.text_edit_singleline(&mut model.level_path);
    });
    ui.horizontal(|ui| {
        ui.label("Project file");
        ui.text_edit_singleline(&mut model.project_file_path);
    });

    if !model.status.is_empty() {
        ui.colored_label(Color32::from_rgb(220, 140, 80), &model.status);
    }

    let mut play_clicked = false;
    ui.horizontal(|ui| {
        if !is_embedded {
            if ui.button("Ping engine").clicked() {
                match launcher::ping_engine(model.port) {
                    Ok(reply) => {
                        model.status.clear();
                        model.push_log(format!("Ping OK: {reply}"));
                    }
                    Err(e) => {
                        model.status.clone_from(&e);
                        model.push_log(format!("Ping failed: {e}"));
                    }
                }
            }
            if ui.button("Retry start engine").clicked() {
                model.bootstrap_done = false;
                if let Some(mut c) = model.auto_started.take() {
                    let _ = c.kill();
                }
                model.status.clear();
                model.bootstrap_external();
            }
        }
        if ui.button("Open…").clicked() {
            model.open_level_dialog();
        }
        if ui.button("Save").clicked() {
            if let Err(e) = model.save_level_file() {
                model.status.clone_from(&e);
                model.push_log(e);
            } else {
                model.status.clear();
            }
        }
        let play = if is_embedded {
            "▶ Play"
        } else {
            "▶ Play (push to engine)"
        };
        if ui.button(play).clicked() {
            play_clicked = true;
        }
    });

    if play_clicked {
        model.apply_level_to_engine(embedded);
    }

    if is_embedded {
        ui.add_space(4.0);
        let ppp = ui.ctx().pixels_per_point();
        let w = ui.available_width();
        let h = ui.available_height().max(120.0);
        let (_, response) = ui.allocate_exact_size(egui::vec2(w, h), Sense::hover());
        let rect = response.rect;
        // Stroke only: avoid painting an opaque egui layer over the same pixels as the child
        // Vulkan HWND (reduces parent/child "ghost" smear on Windows).
        ui.painter()
            .rect_stroke(rect, 3.0, Stroke::new(1.0, Color32::from_rgb(60, 60, 68)));
        let px = (
            (rect.min.x * ppp).round() as i32,
            (rect.min.y * ppp).round() as i32,
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        );
        debug!(
            target: "vge_embedded",
            rect_points = ?rect,
            pixels_per_point = ppp,
            screen_rect = ?ui.ctx().screen_rect(),
            viewport_px = ?px,
            "Level tab: engine viewport rect (points → physical px for winit child overlay)"
        );
        model.engine_viewport_px = Some(px);
    } else {
        model.engine_viewport_px = None;
    }

    if !is_embedded {
        ui.separator();
        let log_h = ui.available_height().max(80.0);
        egui::ScrollArea::vertical()
            .max_height(log_h)
            .show(ui, |ui| {
                for line in &model.log {
                    ui.monospace(line);
                }
            });
    }
}

fn draw_assets_tab(ui: &mut egui::Ui, model: &mut EditorModel) {
    ui.heading("Asset manager");
    ui.label(egui::RichText::new(
        "Import level JSON, MagicaVoxel .vox models, or Lua .lua scripts. Paths are stored in the level file/project file (project-relative when a .vge project is active). \
         Assign a script to an object from the Scene panel (Lua chunk must return function(dt, api) … end).",
    )
    .weak());
    ui.separator();

    if ui.button("Import files…").clicked() {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            if let Err(e) = model.import_asset_paths(paths) {
                model.status.clone_from(&e);
                model.push_log(e);
            } else {
                model.status.clear();
            }
        }
    }

    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let snapshot: Vec<_> = model.level.assets.clone();
        for a in snapshot {
            ui.horizontal(|ui| {
                ui.label(format!("{:?}", a.kind));
                ui.label(&a.name);
                ui.label(egui::RichText::new(&a.path).small().weak());
                if ui.button("Remove").clicked() {
                    model.remove_asset_by_id(&a.id);
                }
            });
        }
        if model.level.assets.is_empty() {
            ui.label(egui::RichText::new("No assets yet.").weak());
        }
    });
}
