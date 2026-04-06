//! egui layout shared between eframe and embedded runners.

use crate::launcher;
use crate::model::{EditorMainTab, EditorModel, FpsOverlayCorner, VoxelEditPlane, VoxelPaintTool};
use eframe::egui;
use eframe::egui::{
    menu, Button, Color32, FontId, Key, KeyboardShortcut, Modifiers, PointerButton, Sense, Stroke,
};
use engine_core::EngineState;
use scene::{ids, AssetKind, PrefabCategory, PrefabLibrary, TerrainMode};
use std::collections::HashSet;
use std::path::Path;
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
    let is_embedded = embedded.is_some();
    if is_embedded {
        // Preferences are edited in a separate process; poll at a low cadence to
        // pick up changes without re-reading from disk every frame.
        model.reload_preferences_from_disk_if_due();
    }
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
                if ui.button("Preferences…").clicked() {
                    model.open_preferences_window();
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
                if ui.button("Preferences…").clicked() {
                    model.open_preferences_window();
                    ui.close_menu();
                }
                ui.separator();
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
            ui.selectable_value(
                &mut model.main_tab,
                EditorMainTab::ModelEditor,
                "Model Editor",
            );
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
            ui.separator();
            ui.label(egui::RichText::new("Custom").weak());
            if ui.button("User Model").clicked() {
                if let Err(e) = model.add_user_model_from_dialog() {
                    model.status.clone_from(&e);
                    model.push_log(e);
                } else {
                    model.status.clear();
                }
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
                // Keep script assignment changes deferred to avoid mutable borrow conflicts.
                let mut assign_script: Option<Option<String>> = None;
                let mut browse_script = false;
                if let Err(e) = model.ensure_project_scripts_registered() {
                    model.status.clone_from(&e);
                    model.push_log(e);
                }
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
                    ui.horizontal(|ui| {
                        ui.label("sx");
                        ui.add(
                            egui::DragValue::new(&mut o.scale[0])
                                .speed(0.05)
                                .range(0.001..=1000.0),
                        );
                        ui.label("sy");
                        ui.add(
                            egui::DragValue::new(&mut o.scale[1])
                                .speed(0.05)
                                .range(0.001..=1000.0),
                        );
                        ui.label("sz");
                        ui.add(
                            egui::DragValue::new(&mut o.scale[2])
                                .speed(0.05)
                                .range(0.001..=1000.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("rx");
                        ui.add(egui::DragValue::new(&mut o.rotation[0]).speed(0.02));
                        ui.label("ry");
                        ui.add(egui::DragValue::new(&mut o.rotation[1]).speed(0.02));
                        ui.label("rz");
                        ui.add(egui::DragValue::new(&mut o.rotation[2]).speed(0.02));
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
                                assign_script = Some(None);
                            }
                            for (id, name) in &scripts {
                                let chosen = o.script_asset_id.as_deref() == Some(id.as_str());
                                if ui.selectable_label(chosen, name).clicked() {
                                    assign_script = Some(Some(id.clone()));
                                }
                            }
                        });
                    if ui.button("Browse script…").clicked() {
                        browse_script = true;
                    }
                    ui.separator();
                    ui.label("Model (VOX asset)");
                    let vox_assets: Vec<(String, String)> = model
                        .level
                        .assets
                        .iter()
                        .filter(|a| a.kind == AssetKind::Vox)
                        .map(|a| (a.id.clone(), a.name.clone()))
                        .collect();
                    let selected_model_label = o
                        .model_asset_id
                        .as_deref()
                        .and_then(|sid| model.level.assets.iter().find(|a| a.id == sid))
                        .filter(|a| a.kind == AssetKind::Vox)
                        .map(|a| a.name.as_str())
                        .unwrap_or("Built-in prefab");
                    egui::ComboBox::from_id_salt("obj_model_asset")
                        .selected_text(selected_model_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(o.model_asset_id.is_none(), "Built-in prefab")
                                .clicked()
                            {
                                o.model_asset_id = None;
                            }
                            for (id, name) in &vox_assets {
                                let chosen = o.model_asset_id.as_deref() == Some(id.as_str());
                                if ui.selectable_label(chosen, name).clicked() {
                                    o.model_asset_id = Some(id.clone());
                                }
                            }
                        });

                    if o.prefab_id == ids::CAMERA || o.camera.is_some() {
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
                    } else if ui.button("Attach camera rig").clicked() {
                        o.camera = Some(Default::default());
                    }
                }
                if let Some(new_sel) = assign_script {
                    if let Some(obj) = model.level.objects.iter_mut().find(|o| o.instance_id == id)
                    {
                        obj.script_asset_id = new_sel;
                    }
                }
                if browse_script {
                    match model.browse_script_asset_for_object() {
                        Ok(Some(asset_id)) => {
                            if let Some(obj) =
                                model.level.objects.iter_mut().find(|o| o.instance_id == id)
                            {
                                obj.script_asset_id = Some(asset_id);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            model.status.clone_from(&e);
                            model.push_log(e);
                        }
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
        EditorMainTab::ModelEditor => {
            if embedded.is_some() {
                model.engine_viewport_px = None;
            }
            draw_model_editor_tab(ui, model);
        }
    });

    draw_code_editor_window(ctx, model);

    if is_embedded && model.preferences.show_fps_overlay {
        if let Some((vx, vy, vw, vh)) = model.engine_viewport_px {
            let ppp = ctx.pixels_per_point();
            let min_x = vx as f32 / ppp;
            let min_y = vy as f32 / ppp;
            let max_x = (vx as f32 + vw as f32) / ppp;
            let max_y = (vy as f32 + vh as f32) / ppp;
            let pad = 8.0;
            let box_w = 104.0;
            let box_h = 28.0;
            // Embedded engine is a native child window and may occlude egui layers.
            // Draw FPS just outside the viewport bounds so it stays visible.
            let mut pos = match model.preferences.fps_overlay_corner {
                FpsOverlayCorner::TopLeft => egui::pos2(min_x + pad, min_y - box_h - pad),
                FpsOverlayCorner::TopRight => egui::pos2(max_x - box_w - pad, min_y - box_h - pad),
                FpsOverlayCorner::BottomLeft => egui::pos2(min_x + pad, max_y + pad),
                FpsOverlayCorner::BottomRight => egui::pos2(max_x - box_w - pad, max_y + pad),
            };
            let screen = ctx.screen_rect();
            pos.x = pos
                .x
                .clamp(screen.left() + 4.0, screen.right() - box_w - 4.0);
            pos.y = pos
                .y
                .clamp(screen.top() + 4.0, screen.bottom() - box_h - 4.0);
            egui::Area::new("fps_overlay".into())
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    let text = format!("FPS: {:.1}", model.render_fps);
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(16, 16, 20, 180))
                        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(text)
                                    .font(FontId::monospace(14.0))
                                    .color(Color32::from_rgb(180, 255, 180)),
                            );
                        });
                });
        }
    }
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
        ui.label(
            egui::RichText::new(
                "Preview controls: mouse wheel = zoom, middle-drag = orbit, right-drag = pan.",
            )
            .small()
            .weak(),
        );
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
    let mut project_vsync_changed = false;
    if let Some(project) = model.current_project.as_mut() {
        ui.collapsing("Project settings", |ui| {
            project_vsync_changed = ui
                .checkbox(&mut project.vsync_enabled, "Enable VSync")
                .changed();
            ui.label(
                egui::RichText::new("When disabled (default), rendering runs uncapped.")
                    .small()
                    .weak(),
            );
        });
    }
    if project_vsync_changed {
        if let Err(e) = model.save_project_file() {
            model.status.clone_from(&e);
            model.push_log(e);
        } else {
            model.status.clear();
        }
    }

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
        if is_embedded {
            ui.checkbox(&mut model.preview_mode_active, "Preview");
            let play = if model.play_mode_active {
                "■ Stop"
            } else {
                "▶ Play"
            };
            if ui.button(play).clicked() {
                if model.play_mode_active {
                    model.stop_play_mode("Play mode stopped.");
                } else {
                    model.begin_play_mode();
                    play_clicked = true;
                }
            }
        } else if ui.button("▶ Play (push to engine)").clicked() {
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
    ui.heading("Assets / Project files");
    ui.label(
        egui::RichText::new(
            "Browse the current project folder tree. Open files in the in-editor code editor or external editor based on Preferences.",
        )
        .weak(),
    );
    ui.separator();

    let Some(project_root) = model.project_root_dir() else {
        ui.label(egui::RichText::new("Open or create a project to browse files.").weak());
        return;
    };

    ui.horizontal(|ui| {
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
        ui.label(format!(
            "Selected folder: {}",
            model.selected_asset_rel_dir_normalized()
        ));
    });

    ui.horizontal(|ui| {
        ui.label("New file");
        ui.add_sized(
            [200.0, 24.0],
            egui::TextEdit::singleline(&mut model.asset_browser_new_file_name)
                .hint_text("example.lua"),
        );
        if ui.button("Create").clicked() {
            if let Err(e) = model.create_new_file_in_selected_dir() {
                model.status.clone_from(&e);
                model.push_log(e);
            } else {
                model.status.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("New folder");
        ui.add_sized(
            [200.0, 24.0],
            egui::TextEdit::singleline(&mut model.asset_browser_new_folder_name)
                .hint_text("scripts"),
        );
        if ui.button("Create").clicked() {
            if let Err(e) = model.create_new_folder_in_selected_dir() {
                model.status.clone_from(&e);
                model.push_log(e);
            } else {
                model.status.clear();
            }
        }
    });

    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        draw_project_tree(ui, model, &project_root, &project_root);

        ui.separator();
        ui.heading("Registered level assets");
        let snapshot: Vec<_> = model.level.assets.clone();
        for a in snapshot {
            ui.horizontal(|ui| {
                ui.label(format!("{:?}", a.kind));
                ui.label(&a.name);
                ui.label(egui::RichText::new(&a.path).small().weak());
                if a.kind == AssetKind::Vox && ui.button("Add to scene").clicked() {
                    let label = if a.name.trim().is_empty() {
                        "Model"
                    } else {
                        a.name.as_str()
                    };
                    if let Err(e) = model.add_model_instance(&a.id, label) {
                        model.status.clone_from(&e);
                        model.push_log(e);
                    } else {
                        model.status.clear();
                    }
                }
                if ui.button("Remove").clicked() {
                    model.remove_asset_by_id(&a.id);
                }
            });
        }
    });
}

fn draw_project_tree(ui: &mut egui::Ui, model: &mut EditorModel, root: &Path, dir: &Path) {
    let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|x| x.path())).collect(),
        Err(_) => return,
    };
    entries.sort();

    for path in entries {
        let rel = match path.strip_prefix(root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<invalid>");
        if path.is_dir() {
            let selected = model.selected_asset_rel_dir_normalized() == rel;
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(selected, format!("📁 {name}"))
                    .clicked()
                {
                    model.asset_browser_selected_rel_dir = rel.clone();
                }
            });
            ui.indent(format!("dir_{rel}"), |ui| {
                draw_project_tree(ui, model, root, &path);
            });
        } else {
            ui.horizontal(|ui| {
                ui.label("📄");
                if ui.button(name).clicked() {
                    if let Err(e) = model.open_asset_file(&path) {
                        model.status.clone_from(&e);
                        model.push_log(e);
                    } else {
                        model.status.clear();
                    }
                }
                ui.label(egui::RichText::new(&rel).small().weak());
            });
        }
    }
}

fn draw_code_editor_window(ctx: &egui::Context, model: &mut EditorModel) {
    if !model.code_editor_open {
        return;
    }
    let mut open = model.code_editor_open;
    egui::Window::new("Code editor")
        .open(&mut open)
        .resizable(true)
        .default_size([900.0, 620.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("File:");
                ui.monospace(&model.code_editor_path);
            });
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    if let Err(e) = model.save_open_code_editor_file() {
                        model.code_editor_status = e;
                    }
                }
                if ui.button("Reload").clicked() {
                    match std::fs::read_to_string(&model.code_editor_path) {
                        Ok(s) => {
                            model.code_editor_text = s;
                            model.code_editor_dirty = false;
                            model.code_editor_status = "Reloaded.".to_string();
                        }
                        Err(e) => {
                            model.code_editor_status =
                                format!("reload {}: {e}", model.code_editor_path);
                        }
                    }
                }
            });
            let resp = ui.add_sized(
                [ui.available_width(), ui.available_height() - 40.0],
                egui::TextEdit::multiline(&mut model.code_editor_text)
                    .desired_rows(30)
                    .lock_focus(true)
                    .code_editor(),
            );
            if resp.changed() {
                model.code_editor_dirty = true;
            }
            if !model.code_editor_status.is_empty() {
                ui.label(
                    egui::RichText::new(&model.code_editor_status)
                        .small()
                        .weak(),
                );
            }
        });
    model.code_editor_open = open;
}

fn draw_model_editor_tab(ui: &mut egui::Ui, model: &mut EditorModel) {
    ui.heading("VOX Model Editor");
    ui.label(
        egui::RichText::new(
            "Generate base voxel prefabs and export MagicaVoxel .vox assets into the project.",
        )
        .weak(),
    );
    ui.label(
        egui::RichText::new(
            "Preview controls: wheel = zoom, drag = rotate model, right-drag = offset pivot center.",
        )
        .small()
        .weak(),
    );
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Model edge");
        ui.add(egui::DragValue::new(&mut model.voxel_model_editor.edge).range(1..=255));
        ui.label("Sphere radius");
        ui.add(egui::DragValue::new(&mut model.voxel_model_editor.sphere_radius).range(1..=32));
        ui.label("Color index");
        ui.add(egui::DragValue::new(&mut model.voxel_model_editor.color_index).range(1..=255));
    });
    ui.horizontal(|ui| {
        if ui.button("Generate Cube").clicked() {
            model.generate_model_cube();
        }
        if ui.button("Generate Sphere (cubes)").clicked() {
            model.generate_model_sphere();
        }
        if ui.button("Clear").clicked() {
            model.clear_model_voxels();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Tool");
        ui.selectable_value(
            &mut model.voxel_model_editor.active_tool,
            VoxelPaintTool::Paint,
            "Brush",
        );
        ui.selectable_value(
            &mut model.voxel_model_editor.active_tool,
            VoxelPaintTool::Erase,
            "Eraser",
        );
        ui.separator();
        ui.label("Edit plane");
        ui.selectable_value(
            &mut model.voxel_model_editor.active_plane,
            VoxelEditPlane::XY,
            "XY",
        );
        ui.selectable_value(
            &mut model.voxel_model_editor.active_plane,
            VoxelEditPlane::XZ,
            "XZ",
        );
        ui.selectable_value(
            &mut model.voxel_model_editor.active_plane,
            VoxelEditPlane::YZ,
            "YZ",
        );
    });
    let edge = model.voxel_model_editor.edge.max(1);
    let max_layer = edge.saturating_sub(1);
    model.voxel_model_editor.active_layer = model.voxel_model_editor.active_layer.min(max_layer);
    ui.horizontal(|ui| {
        ui.label("Layer");
        ui.add(egui::Slider::new(
            &mut model.voxel_model_editor.active_layer,
            0..=max_layer,
        ));
    });

    ui.separator();
    ui.columns(2, |cols| {
        draw_model_preview(&mut cols[0], model);
        draw_slice_paint_panel(&mut cols[1], model);
    });

    ui.horizontal(|ui| {
        ui.label("Export name");
        ui.text_edit_singleline(&mut model.voxel_model_editor.export_name);
        if ui.button("Export .vox…").clicked() {
            match model.export_model_vox_dialog() {
                Ok(Some(_asset_id)) => {}
                Ok(None) => {}
                Err(e) => {
                    model.status.clone_from(&e);
                    model.push_log(e);
                }
            }
        }
    });
    ui.separator();
    ui.label(format!(
        "Current voxel count: {}",
        model.voxel_model_editor.voxels.len()
    ));
}

fn draw_model_preview(ui: &mut egui::Ui, model: &mut EditorModel) {
    ui.label("3D Preview");
    let size = egui::vec2(ui.available_width().max(200.0), 340.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_gray(20));
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(70)));
    if resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Middle) {
        // CAD-style orbit: both primary and middle drag map to yaw/pitch.
        // Use per-frame pointer delta for stable, predictable movement.
        let d = ui.input(|i| i.pointer.delta());
        let orbit_sens = 0.01_f32;
        let sx = if model.preferences.invert_orbit_x {
            1.0
        } else {
            -1.0
        };
        let sy = if model.preferences.invert_orbit_y {
            -1.0
        } else {
            1.0
        };
        model.voxel_model_editor.orbit_yaw += sx * d.x * orbit_sens;
        model.voxel_model_editor.orbit_pitch += sy * d.y * orbit_sens;
        // Keep angles bounded but continuous to allow infinite orbiting.
        let tau = std::f32::consts::TAU;
        model.voxel_model_editor.orbit_yaw =
            (model.voxel_model_editor.orbit_yaw + std::f32::consts::PI).rem_euclid(tau)
                - std::f32::consts::PI;
        model.voxel_model_editor.orbit_pitch =
            (model.voxel_model_editor.orbit_pitch + std::f32::consts::PI).rem_euclid(tau)
                - std::f32::consts::PI;
        ui.ctx().request_repaint();
    }
    if resp.dragged_by(PointerButton::Secondary) {
        let d = resp.drag_delta();
        // Right-drag moves the pivot center in model-space so later rotations orbit around it.
        let edge_now = model.voxel_model_editor.edge.max(1) as f32;
        let scale_now = rect.width().min(rect.height())
            / (edge_now + model.voxel_model_editor.camera_distance * 0.2);
        let model_units_per_px = (1.0 / scale_now.max(0.0001)).clamp(0.001, 10.0);
        model.voxel_model_editor.pan_x += d.x * model_units_per_px;
        model.voxel_model_editor.pan_y -= d.y * model_units_per_px;
        ui.ctx().request_repaint();
    }

    let voxels = model.voxel_model_editor.voxels.clone();
    let occupied: HashSet<(u8, u8, u8)> = voxels.iter().map(|v| (v.x, v.y, v.z)).collect();
    let edge = model.voxel_model_editor.edge.max(1) as f32;
    let pivot_center = if voxels.is_empty() {
        let c = (edge - 1.0) * 0.5;
        [c, c, c]
    } else {
        let mut sx = 0.0f32;
        let mut sy = 0.0f32;
        let mut sz = 0.0f32;
        for v in &voxels {
            sx += v.x as f32;
            sy += v.y as f32;
            sz += v.z as f32;
        }
        let n = voxels.len() as f32;
        [sx / n, sy / n, sz / n]
    };
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let model_size = model.voxel_model_editor.edge.max(1) as f32;
            let model_radius = model_size * 0.5;
            let adaptive_speed =
                (model_radius * 0.06 + model.voxel_model_editor.camera_distance * 0.12).max(0.2);
            model.voxel_model_editor.camera_distance = (model.voxel_model_editor.camera_distance
                - scroll * adaptive_speed * 0.03)
                .clamp(1.0, 1000.0);
            ui.ctx().request_repaint();
        }
    }
    let sy = model.voxel_model_editor.orbit_yaw.sin();
    let cy = model.voxel_model_editor.orbit_yaw.cos();
    let sx = model.voxel_model_editor.orbit_pitch.sin();
    let cx = model.voxel_model_editor.orbit_pitch.cos();
    let scale =
        rect.width().min(rect.height()) / (edge + model.voxel_model_editor.camera_distance * 0.2);
    let center = rect.center();
    // Fixed point light from upper-left/front relative to model space.
    let normalize3 = |v: [f32; 3]| -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len <= 0.0001 {
            [0.0, 1.0, 0.0]
        } else {
            [v[0] / len, v[1] / len, v[2] / len]
        }
    };
    let dot3 = |a: [f32; 3], b: [f32; 3]| -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] };
    // Fixed key light: upper-left and slightly behind the viewer/camera direction.
    let light_dir = normalize3([-0.55_f32, 0.85_f32, -0.55_f32]);

    let mut projected: Vec<(f32, egui::Pos2, u8, f32, f32)> = voxels
        .iter()
        .map(|v| {
            let sample = |x: i32, y: i32, z: i32| -> f32 {
                if x < 0 || y < 0 || z < 0 || x > 255 || y > 255 || z > 255 {
                    return 0.0;
                }
                if occupied.contains(&(x as u8, y as u8, z as u8)) {
                    1.0
                } else {
                    0.0
                }
            };
            let vx = v.x as i32;
            let vy = v.y as i32;
            let vz = v.z as i32;
            let gx = sample(vx + 1, vy, vz) - sample(vx - 1, vy, vz);
            let gy = sample(vx, vy + 1, vz) - sample(vx, vy - 1, vz);
            let gz = sample(vx, vy, vz + 1) - sample(vx, vy, vz - 1);
            let normal = normalize3([-gx, -gy, -gz]);

            let mut x = v.x as f32 - (pivot_center[0] + model.voxel_model_editor.pan_x);
            let mut y = v.y as f32 - (pivot_center[1] + model.voxel_model_editor.pan_y);
            let mut z = v.z as f32 - pivot_center[2];
            let rx = cy * x + sy * z;
            let rz = -sy * x + cy * z;
            x = rx;
            z = rz;
            let ry = cx * y - sx * z;
            let rz2 = sx * y + cx * z;
            y = ry;
            z = rz2;
            // Rotate normal with the model so a fixed world light changes shading as model rotates.
            let (mut nx, mut ny, mut nz) = (normal[0], normal[1], normal[2]);
            let nrx = cy * nx + sy * nz;
            let nrz = -sy * nx + cy * nz;
            nx = nrx;
            nz = nrz;
            let nry = cx * ny - sx * nz;
            let nrz2 = sx * ny + cx * nz;
            ny = nry;
            nz = nrz2;
            let normal_world = normalize3([nx, ny, nz]);
            let diffuse = dot3(normal_world, light_dir).max(0.0_f32);
            // Brighter baseline with directional contrast preserved:
            // - higher ambient keeps far/back-facing voxels readable
            // - diffuse term still gives clear side-to-side shading
            let lighting: f32 = (0.45_f32 + diffuse * 0.65_f32).clamp(0.30_f32, 1.0_f32);
            let p = egui::pos2(center.x + x * scale, center.y - y * scale);
            (z, p, v.color_index, lighting, y)
        })
        .collect();
    projected.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (z, p, color_idx, lighting, y_world) in projected {
        let shadow_strength: f32 = (0.22_f32 - (z * 0.004_f32)).clamp(0.06_f32, 0.22_f32);
        let shadow_offset = egui::vec2(8.0_f32 + z * 0.03_f32, 8.0_f32 + z * 0.03_f32);
        let shadow_p = p + shadow_offset + egui::vec2(0.0_f32, y_world.max(0.0_f32) * 0.2_f32);
        painter.circle_filled(
            shadow_p,
            (scale * 0.42).max(1.5),
            Color32::from_rgba_unmultiplied(0, 0, 0, (shadow_strength * 255.0) as u8),
        );
        let shade: u8 = color_idx.max(32_u8);
        let r = ((shade as f32) * lighting).clamp(0.0, 255.0) as u8;
        let g = ((shade.saturating_sub(20) as f32) * lighting).clamp(0.0, 255.0) as u8;
        let b = ((shade.saturating_add(10) as f32) * lighting).clamp(0.0, 255.0) as u8;
        let color = Color32::from_rgb(r, g, b);
        painter.circle_filled(p, (scale * 0.42).max(1.5), color);
    }
}

fn draw_slice_paint_panel(ui: &mut egui::Ui, model: &mut EditorModel) {
    ui.label("Paint / Erase");
    let edge = model.voxel_model_editor.edge.max(1);
    let cell = (ui.available_width().min(340.0) / edge as f32).clamp(8.0, 28.0);
    let size = egui::vec2(cell * edge as f32, cell * edge as f32);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::from_gray(14));

    for i in 0..=edge {
        let x = rect.left() + i as f32 * cell;
        let y = rect.top() + i as f32 * cell;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0, Color32::from_gray(40)),
        );
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_gray(40)),
        );
    }

    for v in &model.voxel_model_editor.voxels {
        if v.x >= edge || v.y >= edge || v.z >= edge {
            continue;
        }
        let is_on_layer = match model.voxel_model_editor.active_plane {
            VoxelEditPlane::XY => v.z == model.voxel_model_editor.active_layer,
            VoxelEditPlane::XZ => v.y == model.voxel_model_editor.active_layer,
            VoxelEditPlane::YZ => v.x == model.voxel_model_editor.active_layer,
        };
        if !is_on_layer {
            continue;
        }
        let (a, b) = match model.voxel_model_editor.active_plane {
            VoxelEditPlane::XY => (v.x, v.y),
            VoxelEditPlane::XZ => (v.x, v.z),
            VoxelEditPlane::YZ => (v.y, v.z),
        };
        let min = egui::pos2(
            rect.left() + a as f32 * cell + 1.0,
            rect.top() + (edge.saturating_sub(1).saturating_sub(b)) as f32 * cell + 1.0,
        );
        let max = min + egui::vec2(cell - 2.0, cell - 2.0);
        let c = v.color_index.max(24);
        painter.rect_filled(
            egui::Rect::from_min_max(min, max),
            1.5,
            Color32::from_rgb(c, c.saturating_sub(16), c.saturating_add(8)),
        );
    }

    if (resp.dragged() || resp.clicked()) && resp.contains_pointer() {
        if let Some(pointer) = resp.interact_pointer_pos() {
            let gx = ((pointer.x - rect.left()) / cell).floor() as i32;
            let gy = ((pointer.y - rect.top()) / cell).floor() as i32;
            if gx >= 0 && gy >= 0 && gx < edge as i32 && gy < edge as i32 {
                let a = gx as u8;
                let b = (edge as i32 - 1 - gy) as u8;
                let layer = model.voxel_model_editor.active_layer;
                let (x, y, z) = match model.voxel_model_editor.active_plane {
                    VoxelEditPlane::XY => (a, b, layer),
                    VoxelEditPlane::XZ => (a, layer, b),
                    VoxelEditPlane::YZ => (layer, a, b),
                };
                match model.voxel_model_editor.active_tool {
                    VoxelPaintTool::Paint => model.paint_model_voxel(x, y, z),
                    VoxelPaintTool::Erase => model.erase_model_voxel(x, y, z),
                }
                ui.ctx().request_repaint();
            }
        }
    }
}
