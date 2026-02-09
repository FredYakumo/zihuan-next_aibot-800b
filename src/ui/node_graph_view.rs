use log::{error, info};
use slint::{ModelRc, VecModel, SharedString, ComponentHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

use crate::error::Result;
use crate::node::graph_io::{
    ensure_positions,
    load_graph_definition_from_json,
    NodeGraphDefinition,
};
use crate::node::registry::NODE_REGISTRY;

use crate::ui::graph_window::{
    EdgeCornerVm, EdgeLabelVm, EdgeSegmentVm, EdgeVm, GridLineVm, NodeGraphWindow, NodeTypeVm,
    NodeVm, PortVm, MessageItemVm,
};
use crate::ui::selection::{BoxSelection, SelectionState};
use crate::ui::window_state::{apply_window_state, load_window_state, save_window_state, WindowState};
#[cfg(target_os = "macos")]
use crate::ui::macos_menu::{install_menu, MenuActions};

const GRID_SIZE: f32 = 20.0;
const NODE_WIDTH_CELLS: f32 = 10.0;
const NODE_HEADER_ROWS: f32 = 2.0;
const NODE_MIN_ROWS: f32 = 3.0;
const NODE_PADDING_BOTTOM: f32 = 0.8;
const CANVAS_WIDTH: f32 = 1200.0;
const CANVAS_HEIGHT: f32 = 800.0;
const EDGE_THICKNESS_RATIO: f32 = 0.3;

use crate::ui::node_render::{InlinePortValue, inline_port_key, get_node_preview_text};

struct GraphTabState {
    id: u64,
    title: String,
    file_path: Option<PathBuf>,
    graph: NodeGraphDefinition,
    selection: SelectionState,
    inline_inputs: HashMap<String, InlinePortValue>,
    is_dirty: bool,
    is_running: bool,
    stop_flag: Option<Arc<AtomicBool>>,
}

fn build_inline_inputs_from_graph(graph: &NodeGraphDefinition) -> HashMap<String, InlinePortValue> {
    let mut map = HashMap::new();
    for node in &graph.nodes {
        for (port_name, val) in &node.inline_values {
            let key = inline_port_key(&node.id, port_name);
            match val {
                serde_json::Value::String(s) => {
                    map.insert(key, InlinePortValue::Text(s.clone()));
                }
                serde_json::Value::Bool(b) => {
                    map.insert(key, InlinePortValue::Bool(*b));
                }
                serde_json::Value::Number(n) => {
                    map.insert(key, InlinePortValue::Text(n.to_string()));
                }
                _ => {}
            }
        }
    }
    map
}

fn tab_display_title(tab: &GraphTabState) -> String {
    if tab.is_dirty {
        format!("{}*", tab.title)
    } else {
        tab.title.clone()
    }
}

fn new_blank_tab(next_untitled: &mut usize, next_id: &mut u64) -> GraphTabState {
    let title = format!("未命名-{}", *next_untitled);
    *next_untitled += 1;
    let id = *next_id;
    *next_id += 1;

    GraphTabState {
        id,
        title,
        file_path: None,
        graph: NodeGraphDefinition::default(),
        selection: SelectionState::default(),
        inline_inputs: HashMap::new(),
        is_dirty: false,
        is_running: false,
        stop_flag: None,
    }
}

fn update_tabs_ui(ui: &NodeGraphWindow, tabs: &[GraphTabState], active_index: usize) {
    let titles: Vec<SharedString> = tabs.iter().map(|t| tab_display_title(t).into()).collect();
    ui.set_graph_tabs(ModelRc::new(VecModel::from(titles)));
    ui.set_active_tab_index(active_index as i32);
}

fn refresh_active_tab_ui(ui: &NodeGraphWindow, tabs: &[GraphTabState], active_index: usize) {
    if let Some(tab) = tabs.get(active_index) {
        apply_graph_to_ui(
            ui,
            &tab.graph,
            Some(tab_display_title(tab)),
            &tab.selection,
            &tab.inline_inputs,
        );
        tab.selection.apply_to_ui(ui);
        ui.set_is_graph_running(tab.is_running);
    }
    update_tabs_ui(ui, tabs, active_index);
}

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>) -> Result<()> {
    register_cjk_fonts();

    let ui = NodeGraphWindow::new()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))?;

    #[cfg(target_os = "macos")]
    ui.set_show_in_window_menu(false);

    if let Some(state) = load_window_state() {
        apply_window_state(&ui.window(), &state);
    }

    let mut next_untitled_index = 1usize;
    let mut next_tab_id = 1u64;

    let mut initial_tab = new_blank_tab(&mut next_untitled_index, &mut next_tab_id);
    if let Some(graph) = initial_graph {
        initial_tab.graph = graph.clone();
        initial_tab.inline_inputs = build_inline_inputs_from_graph(&graph);
        initial_tab.is_dirty = false;
    }

    let tabs = Arc::new(Mutex::new(vec![initial_tab]));
    let active_tab_index = Arc::new(Mutex::new(0usize));
    let next_untitled_index = Arc::new(Mutex::new(next_untitled_index));
    let next_tab_id = Arc::new(Mutex::new(next_tab_id));
    let pending_close_tab_id: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    // Load available node types from registry
    let node_types: Vec<NodeTypeVm> = NODE_REGISTRY
        .get_all_types()
        .iter()
        .map(|meta| NodeTypeVm {
            type_id: meta.type_id.clone().into(),
            display_name: meta.display_name.clone().into(),
            category: meta.category.clone().into(),
            description: meta.description.clone().into(),
        })
        .collect();

    let mut categories: Vec<SharedString> = node_types
        .iter()
        .map(|n| n.category.clone())
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();
    
    ui.set_node_categories(ModelRc::new(VecModel::from(categories)));
    ui.set_available_node_types(ModelRc::new(VecModel::from(node_types.clone())));
    
    let all_node_types = Arc::new(node_types);
    ui.set_grid_size(GRID_SIZE);
    ui.set_edge_thickness(GRID_SIZE * EDGE_THICKNESS_RATIO);

    {
        let tabs_guard = tabs.lock().unwrap();
        let active_index = *active_tab_index.lock().unwrap();
        refresh_active_tab_ui(&ui, &tabs_guard, active_index);
    }

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_open_json(move || {
        let selected_path = match rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .pick_file()
        {
            Some(path) => path,
            None => return,
        };

        if let Ok(graph) = load_graph_definition_from_json(&selected_path) {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.graph = graph.clone();
                tab.inline_inputs = build_inline_inputs_from_graph(&graph);
                tab.selection.clear();
                tab.file_path = Some(selected_path.clone());
                tab.title = selected_path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| selected_path.display().to_string());
                tab.is_dirty = false;
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let save_tab = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        let ui_handle = ui.as_weak();
        move |tab_id: u64| -> bool {
            // Determine file path (may need to show dialog)
            let path = {
                let tabs_guard = tabs_clone.lock().unwrap();
                let tab_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                    Some(index) => index,
                    None => return false,
                };

                tabs_guard[tab_index].file_path.clone()
            };

            let path = if let Some(path) = path {
                path
            } else {
                // Show file dialog without holding lock
                match rfd::FileDialog::new()
                    .add_filter("Node Graph", &["json"])
                    .set_file_name("node_graph.json")
                    .save_file()
                {
                    Some(path) => path,
                    None => return false,
                }
            };

            // Now perform the save with the lock
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let tab_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                Some(index) => index,
                None => return false,
            };

            let tab = &mut tabs_guard[tab_index];
            apply_inline_inputs_to_graph(&mut tab.graph, &tab.inline_inputs);

            if let Err(e) = crate::node::graph_io::save_graph_definition_to_json(&path, &tab.graph) {
                eprintln!("Failed to save graph: {}", e);
                return false;
            }

            tab.file_path = Some(path.clone());
            tab.title = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            tab.is_dirty = false;

            if let Some(ui) = ui_handle.upgrade() {
                let active_index = *active_tab_clone.lock().unwrap();
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }

            true
        }
    });

    let active_tab_clone = Arc::clone(&active_tab_index);
    let tabs_clone = Arc::clone(&tabs);
    let save_tab_clone = Arc::clone(&save_tab);
    ui.on_save_json(move || {
        let tab_id = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            tabs_guard.get(active_index).map(|t| t.id)
        };
        if let Some(tab_id) = tab_id {
            let _ = save_tab_clone(tab_id);
        }
    });

    let close_tab_by_id = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        let next_untitled_index_clone = Arc::clone(&next_untitled_index);
        let next_tab_id_clone = Arc::clone(&next_tab_id);
        let ui_handle = ui.as_weak();
        move |tab_id: u64| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let mut active_index = *active_tab_clone.lock().unwrap();
            let remove_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                Some(index) => index,
                None => return,
            };

            tabs_guard.remove(remove_index);

            if tabs_guard.is_empty() {
                let mut next_untitled = next_untitled_index_clone.lock().unwrap();
                let mut next_id = next_tab_id_clone.lock().unwrap();
                tabs_guard.push(new_blank_tab(&mut *next_untitled, &mut *next_id));
                active_index = 0;
            } else {
                if remove_index < active_index {
                    active_index -= 1;
                } else if remove_index == active_index && active_index >= tabs_guard.len() {
                    active_index = tabs_guard.len() - 1;
                }
            }

            *active_tab_clone.lock().unwrap() = active_index;
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let close_tab_by_id_for_request = Arc::clone(&close_tab_by_id);
    let request_close_tab = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let pending_close_tab_id_for_request = Arc::clone(&pending_close_tab_id);
        let close_tab_by_id_for_request = Arc::clone(&close_tab_by_id_for_request);
        let ui_handle = ui.as_weak();
        move |tab_id: u64| {
            let tabs_guard = tabs_clone.lock().unwrap();
            let tab = match tabs_guard.iter().find(|t| t.id == tab_id) {
                Some(tab) => tab,
                None => return,
            };

            if let Some(ui) = ui_handle.upgrade() {
                if tab.is_running {
                    *pending_close_tab_id_for_request.lock().unwrap() = Some(tab_id);
                    ui.set_show_running_confirm(true);
                    return;
                }

                if tab.is_dirty {
                    *pending_close_tab_id_for_request.lock().unwrap() = Some(tab_id);
                    ui.set_show_save_confirm(true);
                    return;
                }
            }

            close_tab_by_id_for_request(tab_id);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let close_tab_by_id_clone = Arc::clone(&close_tab_by_id);
    ui.on_close_tab(move || {
        let tab_id = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            tabs_guard.get(active_index).map(|tab| tab.id)
        };
        if let Some(tab_id) = tab_id {
            close_tab_by_id_clone(tab_id);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let next_untitled_index_clone = Arc::clone(&next_untitled_index);
    let next_tab_id_clone = Arc::clone(&next_tab_id);
    let ui_handle = ui.as_weak();
    ui.on_new_tab(move || {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let mut next_untitled = next_untitled_index_clone.lock().unwrap();
        let mut next_id = next_tab_id_clone.lock().unwrap();
        tabs_guard.push(new_blank_tab(&mut *next_untitled, &mut *next_id));
        let active_index = tabs_guard.len() - 1;
        *active_tab_clone.lock().unwrap() = active_index;
        if let Some(ui) = ui_handle.upgrade() {
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
        }
    });

    #[cfg(target_os = "macos")]
    {
        let ui_weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
            install_menu(MenuActions {
                open: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_open_json();
                        }
                    }
                }),
                save: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_save_json();
                        }
                    }
                }),
                new_tab: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_new_tab();
                        }
                    }
                }),
                close_tab: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_close_tab();
                        }
                    }
                }),
                quit: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_close_tab();
                        }
                    }
                }),
            });
        });
    }

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_select_tab(move |index: i32| {
        if index < 0 {
            return;
        }
        let tabs_guard = tabs_clone.lock().unwrap();
        let index = index as usize;
        if index >= tabs_guard.len() {
            return;
        }
        *active_tab_clone.lock().unwrap() = index;
        if let Some(ui) = ui_handle.upgrade() {
            refresh_active_tab_ui(&ui, &tabs_guard, index);
        }
    });

    let pending_close_tab_id_for_save = Arc::clone(&pending_close_tab_id);
    let close_tab_by_id_for_save = Arc::clone(&close_tab_by_id);
    let save_tab_for_save = Arc::clone(&save_tab);
    let ui_handle = ui.as_weak();
    ui.on_save_confirm_save(move || {
        if let Some(tab_id) = pending_close_tab_id_for_save.lock().unwrap().take() {
            if save_tab_for_save(tab_id) {
                close_tab_by_id_for_save(tab_id);
            }
        }
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_save_confirm(false);
        }
    });

    let pending_close_tab_id_for_discard = Arc::clone(&pending_close_tab_id);
    let close_tab_by_id_for_discard = Arc::clone(&close_tab_by_id);
    let ui_handle = ui.as_weak();
    ui.on_save_confirm_discard(move || {
        if let Some(tab_id) = pending_close_tab_id_for_discard.lock().unwrap().take() {
            close_tab_by_id_for_discard(tab_id);
        }
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_save_confirm(false);
        }
    });

    let pending_close_tab_id_for_cancel = Arc::clone(&pending_close_tab_id);
    let ui_handle = ui.as_weak();
    ui.on_save_confirm_cancel(move || {
        pending_close_tab_id_for_cancel.lock().unwrap().take();
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_save_confirm(false);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let pending_close_tab_id_for_running = Arc::clone(&pending_close_tab_id);
    let close_tab_by_id_for_running = Arc::clone(&close_tab_by_id);
    let ui_handle = ui.as_weak();
    ui.on_running_confirm_close(move || {
        let tab_id = match pending_close_tab_id_for_running.lock().unwrap().take() {
            Some(tab_id) => tab_id,
            None => return,
        };

        {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                if let Some(stop_flag) = tab.stop_flag.as_ref() {
                    info!("请求停止节点图执行");
                    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }

        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_running_confirm(false);
        }

        let tabs_guard = tabs_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.iter().find(|t| t.id == tab_id) {
            if tab.is_dirty {
                if let Some(ui) = ui_handle.upgrade() {
                    *pending_close_tab_id_for_running.lock().unwrap() = Some(tab_id);
                    ui.set_show_save_confirm(true);
                }
            } else {
                close_tab_by_id_for_running(tab_id);
            }
        }
    });

    let pending_close_tab_id_for_running_cancel = Arc::clone(&pending_close_tab_id);
    let ui_handle = ui.as_weak();
    ui.on_running_confirm_cancel(move || {
        pending_close_tab_id_for_running_cancel.lock().unwrap().take();
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_running_confirm(false);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_add_node(move |type_id: SharedString| {
        let type_id_str = type_id.as_str();
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Err(e) = add_node_to_graph(&mut tab.graph, type_id_str) {
                eprintln!("Failed to add node: {}", e);
                return;
            }
            tab.is_dirty = true;
        }

        if let Some(ui) = ui_handle.upgrade() {
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_run_graph(move || {
        let (tab_id, graph_def, inline_inputs_map) = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let tab = match tabs_guard.get(active_index) {
                Some(tab) => tab,
                None => return,
            };

            if tab.is_running {
                info!("节点图已在运行中");
                return;
            }

            (tab.id, tab.graph.clone(), tab.inline_inputs.clone())
        };

        let mut graph_def = graph_def;

        // Set up inline values in global context for string_data nodes
        {
            use crate::node::util_nodes::STRING_DATA_CONTEXT;
            let mut context = STRING_DATA_CONTEXT.write().unwrap();
            context.clear();

            for node in &graph_def.nodes {
                if node.node_type == "string_data" {
                    let key = inline_port_key(&node.id, "text");
                    if let Some(InlinePortValue::Text(value)) = inline_inputs_map.get(&key) {
                        context.insert(node.id.clone(), value.clone());
                    }
                }
            }
        }

        apply_inline_inputs_to_graph(&mut graph_def, &inline_inputs_map);

        match crate::node::registry::build_node_graph_from_definition(&graph_def) {
            Ok(mut node_graph) => {
                info!("开始执行节点图...");

                let has_event_producer = node_graph
                    .nodes
                    .values()
                    .any(|node| node.node_type() == crate::node::NodeType::EventProducer);

                if has_event_producer {
                    let stop_flag = node_graph.get_stop_flag();

                    {
                        let mut tabs_guard = tabs_clone.lock().unwrap();
                        if let Some(tab) = tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                            tab.is_running = true;
                            tab.stop_flag = Some(stop_flag.clone());
                        }
                    }

                    if let Some(ui) = ui_handle.upgrade() {
                        let active_index = *active_tab_clone.lock().unwrap();
                        let tabs_guard = tabs_clone.lock().unwrap();
                        if let Some(tab) = tabs_guard.get(active_index) {
                            if tab.id == tab_id {
                                ui.set_is_graph_running(true);
                                ui.set_connection_status("⏳ 节点图运行中...".into());
                            }
                        }
                    }

                    let tabs_cb = Arc::clone(&tabs_clone);
                    let ui_weak_cb = ui_handle.clone();
                    let active_tab_cb = Arc::clone(&active_tab_clone);
                    let inline_inputs_cb = inline_inputs_map.clone();

                    node_graph.set_execution_callback(move |node_id, inputs, outputs| {
                        let node_id = node_id.to_string();
                        let mut result = inputs.clone();
                        for (k, v) in outputs {
                            result.insert(k.clone(), v.clone());
                        }

                        let tabs_cb = Arc::clone(&tabs_cb);
                        let ui_weak_cb = ui_weak_cb.clone();
                        let active_tab_cb = Arc::clone(&active_tab_cb);
                        let inline_inputs_cb = inline_inputs_cb.clone();

                        let _ = slint::invoke_from_event_loop(move || {
                            let mut tabs_guard = tabs_cb.lock().unwrap();
                            let active_index = *active_tab_cb.lock().unwrap();
                            let active_tab_id = tabs_guard.get(active_index).map(|t| t.id);
                            if let Some(tab) = tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                                tab.graph.execution_results.insert(node_id, result);
                                if let Some(ui) = ui_weak_cb.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_cb,
                                        );
                                    }
                                }
                            }
                        });
                    });

                    let ui_weak = ui_handle.clone();
                    let tabs_bg = Arc::clone(&tabs_clone);
                    let active_tab_bg = Arc::clone(&active_tab_clone);
                    let inline_inputs_bg = inline_inputs_map.clone();

                    std::thread::spawn(move || {
                        let execution_result = node_graph.execute_and_capture_results();

                        let _ = slint::invoke_from_event_loop(move || {
                            let mut tabs_guard = tabs_bg.lock().unwrap();
                            let active_index = *active_tab_bg.lock().unwrap();
                            let active_tab_id = tabs_guard.get(active_index).map(|t| t.id);
                            let tab = match tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                                Some(tab) => tab,
                                None => return,
                            };

                            tab.graph.execution_results = execution_result.node_results;

                            if let (Some(error_node_id), Some(error_msg)) =
                                (execution_result.error_node_id.clone(), execution_result.error_message.clone())
                            {
                                error!("节点图执行失败: {}", error_msg);
                                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == error_node_id) {
                                    node.has_error = true;
                                }

                                if let Some(ui) = ui_weak.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_bg,
                                        );
                                        ui.invoke_show_error(format!("执行错误：{}", error_msg).into());
                                        ui.set_connection_status(format!("❌ 执行失败: {}", error_msg).into());
                                    }
                                }
                            } else {
                                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                                    info!("节点图执行已停止");
                                } else {
                                    info!("节点图执行成功!");
                                }

                                for node in &mut tab.graph.nodes {
                                    node.has_error = false;
                                }

                                if let Some(ui) = ui_weak.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                                            ui.set_connection_status("⏹ 节点图执行已停止".into());
                                        } else {
                                            ui.set_connection_status("✓ 节点图执行成功".into());
                                        }
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_bg,
                                        );
                                    }
                                }
                            }

                            tab.is_running = false;
                            tab.stop_flag = None;

                            if let Some(ui) = ui_weak.upgrade() {
                                if active_tab_id == Some(tab_id) {
                                    ui.set_is_graph_running(false);
                                }
                            }
                        });
                    });
                } else {
                    let execution_result = node_graph.execute_and_capture_results();

                    let mut tabs_guard = tabs_clone.lock().unwrap();
                    let active_index = *active_tab_clone.lock().unwrap();
                    let active_tab_id = tabs_guard.get(active_index).map(|t| t.id);
                    let tab = match tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                        Some(tab) => tab,
                        None => return,
                    };

                    tab.graph.execution_results = execution_result.node_results;

                    if let (Some(error_node_id), Some(error_msg)) =
                        (execution_result.error_node_id.clone(), execution_result.error_message.clone())
                    {
                        error!("节点图执行失败: {}", error_msg);
                        if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == error_node_id) {
                            node.has_error = true;
                        }

                        if let Some(ui) = ui_handle.upgrade() {
                            if active_tab_id == Some(tab_id) {
                                apply_graph_to_ui(
                                    &ui,
                                    &tab.graph,
                                    Some(tab_display_title(tab)),
                                    &tab.selection,
                                    &inline_inputs_map,
                                );
                                ui.invoke_show_error(format!("执行错误：{}", error_msg).into());
                            }
                        }
                    } else {
                        info!("节点图执行成功!");
                        for node in &mut tab.graph.nodes {
                            node.has_error = false;
                        }

                        if let Some(ui) = ui_handle.upgrade() {
                            if active_tab_id == Some(tab_id) {
                                ui.set_connection_status("✓ 节点图执行成功".into());
                                apply_graph_to_ui(
                                    &ui,
                                    &tab.graph,
                                    Some(tab_display_title(tab)),
                                    &tab.selection,
                                    &inline_inputs_map,
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("构建节点图失败: {}", e);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.invoke_show_error(format!("构建节点图失败：{}", e).into());
                }
            }
        }
    });

    // Add stop graph callback
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_stop_graph(move || {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Some(stop_flag) = tab.stop_flag.as_ref() {
                info!("请求停止节点图执行");
                stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    ui.on_filter_nodes(move |search_text: SharedString, category: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let search_text = search_text.as_str().to_lowercase();
            let category = category.as_str();

            let filtered: Vec<NodeTypeVm> = all_node_types_clone
                .iter()
                .filter(|n| {
                    let name_match = search_text.is_empty() 
                        || n.display_name.to_lowercase().contains(&search_text) 
                        || n.description.to_lowercase().contains(&search_text);
                    let cat_match = category.is_empty() || n.category == category;
                    name_match && cat_match
                })
                .cloned()
                .collect();
            
            ui.set_available_node_types(ModelRc::new(VecModel::from(filtered)));
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    ui.on_show_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_available_node_types(ModelRc::new(VecModel::from(all_node_types_clone.as_ref().clone())));
            ui.set_show_node_selector(true);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_node_selector(false);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_show_error(move |message: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_error_dialog_message(message);
            ui.set_show_error_dialog(true);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_error(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_error_dialog(false);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_moved(move |node_id: SharedString, x: f32, y: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                if let Some(pos) = &mut node.position {
                    pos.x = x;
                    pos.y = y;
                } else {
                    node.position = Some(crate::node::graph_io::GraphPosition { x, y });
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                let edges = build_edges(&tab.graph, &tab.selection, false);
                let (edge_segments, edge_corners, edge_labels) =
                    build_edge_segments(&tab.graph, false);

                ui.set_edges(ModelRc::new(VecModel::from(edges)));
                ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
                ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
                ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_resized(move |node_id: SharedString, width: f32, height: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                node.size = Some(crate::node::graph_io::GraphSize { width, height });
            }

            if let Some(ui) = ui_handle.upgrade() {
                let edges = build_edges(&tab.graph, &tab.selection, false);
                let (edge_segments, edge_corners, edge_labels) =
                    build_edge_segments(&tab.graph, false);

                ui.set_edges(ModelRc::new(VecModel::from(edges)));
                ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
                ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
                ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_move_finished(move |node_id: SharedString, x: f32, y: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let snapped_x = snap_to_grid(x);
            let snapped_y = snap_to_grid(y);
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                if let Some(pos) = &mut node.position {
                    pos.x = snapped_x;
                    pos.y = snapped_y;
                } else {
                    node.position = Some(crate::node::graph_io::GraphPosition {
                        x: snapped_x,
                        y: snapped_y,
                    });
                }
            }

            tab.is_dirty = true;

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_resize_finished(move |node_id: SharedString, width: f32, height: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let snapped_width = snap_to_grid(width).max(GRID_SIZE * NODE_WIDTH_CELLS);
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                let min_height = GRID_SIZE
                    * (NODE_MIN_ROWS
                        .max(NODE_HEADER_ROWS + node
                            .input_ports
                            .len()
                            .max(node.output_ports.len()) as f32)
                        + NODE_PADDING_BOTTOM);
                let snapped_height = snap_to_grid(height).max(min_height);
                node.size = Some(crate::node::graph_io::GraphSize {
                    width: snapped_width,
                    height: snapped_height,
                });
            }

            tab.is_dirty = true;

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let port_selection = Arc::new(Mutex::new(None::<(String, String, bool)>));
    let port_selection_for_click = Arc::clone(&port_selection);
    let port_selection_for_move = Arc::clone(&port_selection);
    let port_selection_for_cancel = Arc::clone(&port_selection);
    let ui_handle_for_click = ui.as_weak();
    let ui_handle_for_move = ui.as_weak();
    let ui_handle_for_cancel = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);

    ui.on_port_clicked(move |node_id: SharedString, port_name: SharedString, is_input: bool| {
        let node_id_str = node_id.to_string();
        let port_name_str = port_name.to_string();

        let mut selection = port_selection_for_click.lock().unwrap();

        if let Some((prev_node, prev_port, prev_is_input)) = selection.take() {
            if prev_is_input != is_input {
                let mut tabs_guard = tabs_clone.lock().unwrap();
                let active_index = *active_tab_clone.lock().unwrap();
                if let Some(tab) = tabs_guard.get_mut(active_index) {
                    ensure_positions(&mut tab.graph);

                    let (from_node, from_port, to_node, to_port) = if is_input {
                        (prev_node, prev_port, node_id_str, port_name_str)
                    } else {
                        (node_id_str, port_name_str, prev_node, prev_port)
                    };

                    tab.graph.edges.push(crate::node::graph_io::EdgeDefinition {
                        from_node_id: from_node,
                        from_port,
                        to_node_id: to_node,
                        to_port,
                    });

                    tab.is_dirty = true;

                    if let Some(ui) = ui_handle_for_click.upgrade() {
                        ui.set_drag_line_visible(false);
                        ui.set_show_port_hint(false);
                        ui.set_port_hint_text("".into());
                        refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                    }
                }
            } else {
                *selection = Some((prev_node, prev_port, prev_is_input));
            }
        } else {
            *selection = Some((node_id_str.clone(), port_name_str.clone(), is_input));
            if let Some(ui) = ui_handle_for_click.upgrade() {
                let mut tabs_guard = tabs_clone.lock().unwrap();
                let active_index = *active_tab_clone.lock().unwrap();
                if let Some(tab) = tabs_guard.get_mut(active_index) {
                    ensure_positions(&mut tab.graph);
                    if let Some((from_x, from_y)) = get_port_center(
                        &tab.graph,
                        node_id_str.as_str(),
                        port_name_str.as_str(),
                        is_input,
                    ) {
                        ui.set_drag_line_visible(true);
                        ui.set_drag_line_from_x(from_x);
                        ui.set_drag_line_from_y(from_y);
                        ui.set_drag_line_to_x(from_x);
                        ui.set_drag_line_to_y(from_y);
                    }
                }

                if is_input {
                    ui.set_port_hint_text("连接到输出port,按右键取消".into());
                } else {
                    ui.set_port_hint_text("连接到输入port,按右键取消".into());
                }
                ui.set_show_port_hint(true);
            }
        }
    });

    ui.on_pointer_moved(move |x: f32, y: f32| {
        if port_selection_for_move.lock().unwrap().is_none() {
            return;
        }

        if let Some(ui) = ui_handle_for_move.upgrade() {
            ui.set_drag_line_to_x(snap_to_grid_center(x));
            ui.set_drag_line_to_y(snap_to_grid_center(y));
        }
    });

    ui.on_cancel_connect(move || {
        *port_selection_for_cancel.lock().unwrap() = None;
        if let Some(ui) = ui_handle_for_cancel.upgrade() {
            ui.set_drag_line_visible(false);
            ui.set_show_port_hint(false);
            ui.set_port_hint_text("".into());
        }
    });

    // Selection callbacks for active tab
    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_clicked(move |node_id: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.selection.select_node(node_id.to_string(), false);
                tab.selection.apply_to_ui(&ui);
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                );
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_edge_clicked(move |from_node: SharedString, from_port: SharedString, to_node: SharedString, to_port: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.selection.select_edge(
                    from_node.to_string(),
                    from_port.to_string(),
                    to_node.to_string(),
                    to_port.to_string(),
                );
                tab.selection.apply_to_ui(&ui);
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                );
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_canvas_clicked(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.selection.clear();
                tab.selection.apply_to_ui(&ui);
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                );
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_delete_selected(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if !tab.selection.selected_node_ids.is_empty() {
                    tab.graph.nodes.retain(|n| !tab.selection.selected_node_ids.contains(&n.id));
                    tab.graph.edges.retain(|e| {
                        !tab.selection.selected_node_ids.contains(&e.from_node_id)
                            && !tab.selection.selected_node_ids.contains(&e.to_node_id)
                    });
                } else if !tab.selection.selected_edge_from_node.is_empty() {
                    tab.graph.edges.retain(|e| {
                        !(e.from_node_id == tab.selection.selected_edge_from_node
                            && e.from_port == tab.selection.selected_edge_from_port
                            && e.to_node_id == tab.selection.selected_edge_to_node
                            && e.to_port == tab.selection.selected_edge_to_port)
                    });
                }

                tab.selection.clear();
                tab.selection.apply_to_ui(&ui);
                tab.is_dirty = true;

                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });
    
    // Setup box selection
    let box_selection = Arc::new(Mutex::new(BoxSelection::new()));
    
    let ui_handle = ui.as_weak();
    let box_selection_clone = Arc::clone(&box_selection);
    ui.on_box_selection_start(move |x: f32, y: f32| {
        let mut box_sel = box_selection_clone.lock().unwrap();
        box_sel.start(x, y);
        
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_box_selection_visible(true);
            ui.set_box_selection_x(x);
            ui.set_box_selection_y(y);
            ui.set_box_selection_width(0.0);
            ui.set_box_selection_height(0.0);
        }
    });
    
    let ui_handle = ui.as_weak();
    let box_selection_clone = Arc::clone(&box_selection);
    ui.on_box_selection_update(move |x: f32, y: f32| {
        let mut box_sel = box_selection_clone.lock().unwrap();
        box_sel.update(x, y);
        
        if let Some(ui) = ui_handle.upgrade() {
            let (min_x, min_y, max_x, max_y) = box_sel.get_bounds();
            ui.set_box_selection_x(min_x);
            ui.set_box_selection_y(min_y);
            ui.set_box_selection_width(max_x - min_x);
            ui.set_box_selection_height(max_y - min_y);
        }
    });
    
    let ui_handle = ui.as_weak();
    let box_selection_clone = Arc::clone(&box_selection);
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_box_selection_end(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut box_sel = box_selection_clone.lock().unwrap();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();

            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let mut selected_nodes = Vec::new();
                for node in &tab.graph.nodes {
                    if let Some(pos) = &node.position {
                        let (node_width, node_height) = node_dimensions(node);
                        if box_sel.contains_rect(pos.x, pos.y, node_width, node_height) {
                            selected_nodes.push(node.id.clone());
                        }
                    }
                }

                tab.selection.clear();
                for node_id in selected_nodes {
                    tab.selection.select_node(node_id, true);
                }
                tab.selection.apply_to_ui(&ui);

                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                );
            }

            box_sel.end();
            ui.set_box_selection_visible(false);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_inline_port_text_changed(move |node_id: SharedString, port_name: SharedString, value: SharedString| {
        let key = inline_port_key(node_id.as_str(), port_name.as_str());
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            tab.inline_inputs
                .insert(key, InlinePortValue::Text(value.to_string()));
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_inline_port_bool_changed(move |node_id: SharedString, port_name: SharedString, value: bool| {
        let key = inline_port_key(node_id.as_str(), port_name.as_str());
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            tab.inline_inputs.insert(key, InlinePortValue::Bool(value));
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let run_result = ui.run();
    if run_result.is_ok() {
        let state = WindowState::from_window(&ui.window());
        if let Err(e) = save_window_state(&state) {
            eprintln!("Failed to save window state: {e}");
        }
    }

    run_result.map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))
}

fn register_cjk_fonts() {
    use slint::fontique_07::{fontique, shared_collection};
    use std::sync::Arc;

    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/Library/Fonts/NotoSansCJKsc-Regular.otf",
        "/Library/Fonts/NotoSansCJK-Regular.ttc",
    ];

    let mut collection = shared_collection();

    for path in candidates {
        if !Path::new(path).exists() {
            continue;
        }

        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        let blob = fontique::Blob::new(Arc::new(bytes));
        let fonts = collection.register_fonts(blob, None);
        if fonts.is_empty() {
            continue;
        }

        let ids: Vec<_> = fonts.iter().map(|font| font.0).collect();
        let hani = fontique::FallbackKey::new("Hani", None);
        let hira = fontique::FallbackKey::new("Hira", None);
        let kana = fontique::FallbackKey::new("Kana", None);

        collection.append_fallbacks(hani, ids.iter().copied());
        collection.append_fallbacks(hira, ids.iter().copied());
        collection.append_fallbacks(kana, ids.iter().copied());
    }
}

fn apply_graph_to_ui(
    ui: &NodeGraphWindow,
    graph: &NodeGraphDefinition,
    current_file: Option<String>,
    selection_state: &crate::ui::selection::SelectionState,
    inline_inputs: &HashMap<String, InlinePortValue>,
) {
    let mut graph = graph.clone();
    ensure_positions(&mut graph);

    for node in &mut graph.nodes {
        if let Some(pos) = &mut node.position {
            pos.x = snap_to_grid(pos.x);
            pos.y = snap_to_grid(pos.y);
        }
    }
    
    let nodes: Vec<NodeVm> = graph
        .nodes
        .iter()
        .map(|node| {
            let position = node.position.as_ref();
            let (node_width, node_height) = node_dimensions(node);
            let label = format!("{}", node.name);
            let is_selected = selection_state.selected_node_ids.contains(&node.id);
            let preview_text = get_node_preview_text(
                &node.id,
                &node.node_type,
                &graph,
                inline_inputs,
            );
            
            let input_ports: Vec<PortVm> = node
                .input_ports
                .iter()
                .map(|p| {
                    let is_connected = graph.edges.iter().any(|e| {
                        e.to_node_id == node.id && e.to_port == p.name
                    });
                    
                    let key = inline_port_key(&node.id, &p.name);
                    let (inline_text, inline_bool, has_inline) = match &p.data_type {
                        crate::node::DataType::Boolean => {
                            let value = match inline_inputs.get(&key) {
                                Some(InlinePortValue::Bool(v)) => *v,
                                Some(InlinePortValue::Text(v)) => v.eq_ignore_ascii_case("true"),
                                None => false,
                            };
                            (String::new(), value, true) // Boolean inputs always have a value (default false)
                        }
                        crate::node::DataType::String
                        | crate::node::DataType::Integer
                        | crate::node::DataType::Float => {
                            let value = match inline_inputs.get(&key) {
                                Some(InlinePortValue::Text(v)) => v.clone(),
                                Some(InlinePortValue::Bool(v)) => v.to_string(),
                                None => String::new(),
                            };
                            let has_val = !value.is_empty();
                            (value, false, has_val)
                        }
                        _ => (String::new(), false, false),
                    };
                    PortVm {
                        name: p.name.clone().into(),
                        is_input: true,
                        is_connected,
                        is_required: p.required,
                        has_value: has_inline,
                        data_type: p.data_type.to_string().into(),
                        inline_text: inline_text.into(),
                        inline_bool,
                    }
                })
                .collect();
            
            let output_ports: Vec<PortVm> = node
                .output_ports
                .iter()
                .map(|p| {
                    let is_connected = graph.edges.iter().any(|e| {
                        e.from_node_id == node.id && e.from_port == p.name
                    });
                    PortVm {
                        name: p.name.clone().into(),
                        is_input: false,
                        is_connected,
                        is_required: false,
                        has_value: false,
                        data_type: p.data_type.to_string().into(),
                        inline_text: "".into(),
                        inline_bool: false,
                    }
                })
                .collect();

            // Get string_data text value from inline inputs
            let string_data_text = if node.node_type == "string_data" {
                let key = inline_port_key(&node.id, "text");
                match inline_inputs.get(&key) {
                    Some(InlinePortValue::Text(value)) => value.clone(),
                    _ => String::new(),
                }
            } else {
                String::new()
            };

            // Get message list for preview_message_list nodes
            let message_list = if node.node_type == "preview_message_list" {
                use crate::ui::node_render::preview_message_list::get_message_list_data;
                get_message_list_data(&node.id, &graph)
                    .into_iter()
                    .map(|msg| MessageItemVm {
                        role: msg.role.into(),
                        content: msg.content.into(),
                    })
                    .collect()
            } else {
                Vec::new()
            };

            NodeVm {
                id: node.id.clone().into(),
                label: label.into(),
                preview_text: preview_text.into(),
                node_type: node.node_type.clone().into(),
                string_data_text: string_data_text.into(),
                message_list: ModelRc::new(VecModel::from(message_list)),
                x: position.map(|p| snap_to_grid(p.x)).unwrap_or(0.0),
                y: position.map(|p| snap_to_grid(p.y)).unwrap_or(0.0),
                width: node_width,
                height: node_height,
                input_ports: ModelRc::new(VecModel::from(input_ports)),
                output_ports: ModelRc::new(VecModel::from(output_ports)),
                is_selected,
                has_error: node.has_error,
            }
        })
        .collect();

    // Calculate edge visual positions based on node positions
    let edges = build_edges(&graph, selection_state, true);
    let (edge_segments, edge_corners, edge_labels) = build_edge_segments(&graph, true);

    let label = current_file.unwrap_or_else(|| "已加载 JSON".to_string());
    let grid_lines = build_grid_lines(CANVAS_WIDTH, CANVAS_HEIGHT, GRID_SIZE);

    ui.set_nodes(ModelRc::new(VecModel::from(nodes)));
    ui.set_edges(ModelRc::new(VecModel::from(edges)));
    ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
    ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
    ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
    ui.set_grid_lines(ModelRc::new(VecModel::from(grid_lines)));
    ui.set_current_file(label.into());
}

fn apply_inline_inputs_to_graph(
    graph: &mut NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
) {
    for node in &mut graph.nodes {
        for port in &node.input_ports {
            let key = inline_port_key(&node.id, &port.name);
            if let Some(val) = inline_inputs.get(&key) {
                match val {
                    InlinePortValue::Text(s) => {
                        node.inline_values
                            .insert(port.name.clone(), serde_json::Value::String(s.clone()));
                    }
                    InlinePortValue::Bool(b) => {
                        node.inline_values
                            .insert(port.name.clone(), serde_json::Value::Bool(*b));
                    }
                }
            }
        }
    }
}

fn add_node_to_graph(graph: &mut NodeGraphDefinition, type_id: &str) -> Result<()> {
    let id = next_node_id(graph);
    
    // Get metadata from registry
    let all_types = NODE_REGISTRY.get_all_types();
    let metadata = all_types.iter().find(|meta| meta.type_id == type_id);
    
    let display_name = metadata
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| "NewNode".to_string());

    // Create a dummy node instance to get port information
    let dummy_node = NODE_REGISTRY.create_node(type_id, &id, &display_name)?;
    
    graph.nodes.push(crate::node::graph_io::NodeDefinition {
        id,
        name: display_name,
        description: dummy_node.description().map(|s| s.to_string()),
        node_type: type_id.to_string(),
        input_ports: dummy_node.input_ports(),
        output_ports: dummy_node.output_ports(),
        position: None,
        size: None,
        inline_values: HashMap::new(),
        has_error: false,
    });
    
    Ok(())
}

fn next_node_id(graph: &NodeGraphDefinition) -> String {
    let mut index = 1usize;
    loop {
        let candidate = format!("node_{index}");
        if !graph.nodes.iter().any(|node| node.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn find_port_at(
    graph: &NodeGraphDefinition,
    x: f32,
    y: f32,
) -> Option<(String, String, bool)> {
    let port_size = GRID_SIZE;
    let radius = port_size / 2.0;
    let radius_sq = radius * radius;

    let input_center_x = GRID_SIZE * 0.5;
    let base_y_offset = GRID_SIZE * NODE_HEADER_ROWS;

    for node in &graph.nodes {
        let position = match node.position.as_ref() {
            Some(pos) => pos,
            None => continue,
        };

        let (node_width, _) = node_dimensions(node);

        let node_x = position.x;
        let node_y = position.y;

        for (index, port) in node.input_ports.iter().enumerate() {
            let center_x = node_x + input_center_x;
            let center_y = node_y + base_y_offset + index as f32 * GRID_SIZE + radius;

            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_sq {
                return Some((node.id.clone(), port.name.clone(), true));
            }
        }

        for (index, port) in node.output_ports.iter().enumerate() {
            let center_x = node_x + node_width - (GRID_SIZE * 0.5);
            let center_y = node_y + base_y_offset + index as f32 * GRID_SIZE + radius;

            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_sq {
                return Some((node.id.clone(), port.name.clone(), false));
            }
        }
    }

    None
}

fn get_port_center(
    graph: &NodeGraphDefinition,
    node_id: &str,
    port_name: &str,
    is_input: bool,
) -> Option<(f32, f32)> {
    let node = graph.nodes.iter().find(|n| n.id == node_id)?;
    get_port_center_for_node(node, port_name, is_input)
}

fn get_port_center_for_node(
    node: &crate::node::graph_io::NodeDefinition,
    port_name: &str,
    is_input: bool,
) -> Option<(f32, f32)> {
    let position = node.position.as_ref()?;

    let ports = if is_input {
        &node.input_ports
    } else {
        &node.output_ports
    };

    let index = ports.iter().position(|p| p.name == port_name)? as f32;
    let radius = GRID_SIZE / 2.0;
    let base_y_offset = GRID_SIZE * NODE_HEADER_ROWS;
    let (node_width, _) = node_dimensions(node);

    let center_x = if is_input {
        position.x + GRID_SIZE * 0.5
    } else {
        position.x + node_width - (GRID_SIZE * 0.5)
    };
    let center_y = position.y + base_y_offset + index * GRID_SIZE + radius;

    Some((center_x, center_y))
}

fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_SIZE).round() * GRID_SIZE
}

fn snap_to_grid_center(value: f32) -> f32 {
    snap_to_grid(value - GRID_SIZE / 2.0) + GRID_SIZE / 2.0
}


fn route_edge(
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    thickness: f32,
    edge_index: i32,
    snap: bool,
    segments: &mut Vec<EdgeSegmentVm>,
    corners: &mut Vec<EdgeCornerVm>,
) -> (f32, f32) {
    let min_dist = GRID_SIZE * 2.0;

    if to_x < from_x + min_dist {
        // Complex 5-segment route
        let mid_y = (from_y + to_y) / 2.0;
        let x_right = from_x + GRID_SIZE;
        let x_left = to_x - GRID_SIZE;

        let (mid_y, x_right, x_left) = if snap {
            (
                snap_to_grid_center(mid_y),
                snap_to_grid_center(x_right),
                snap_to_grid_center(x_left),
            )
        } else {
            (mid_y, x_right, x_left)
        };

        push_segment(segments, from_x, from_y, x_right, from_y, thickness, edge_index);
        push_segment(segments, x_right, from_y, x_right, mid_y, thickness, edge_index);
        push_segment(segments, x_right, mid_y, x_left, mid_y, thickness, edge_index);
        push_segment(segments, x_left, mid_y, x_left, to_y, thickness, edge_index);
        push_segment(segments, x_left, to_y, to_x, to_y, thickness, edge_index);

        corners.push(EdgeCornerVm { x: x_right, y: from_y, edge_index });
        corners.push(EdgeCornerVm { x: x_right, y: mid_y, edge_index });
        corners.push(EdgeCornerVm { x: x_left, y: mid_y, edge_index });
        corners.push(EdgeCornerVm { x: x_left, y: to_y, edge_index });

        ((x_right + x_left) / 2.0, mid_y)
    } else {
        // Simple 3-segment route
        let mid_x = if snap {
            snap_to_grid_center((from_x + to_x) / 2.0)
        } else {
            (from_x + to_x) / 2.0
        };

        push_segment(segments, from_x, from_y, mid_x, from_y, thickness, edge_index);
        push_segment(segments, mid_x, from_y, mid_x, to_y, thickness, edge_index);
        push_segment(segments, mid_x, to_y, to_x, to_y, thickness, edge_index);
        
        corners.push(EdgeCornerVm { x: mid_x, y: from_y, edge_index });
        corners.push(EdgeCornerVm { x: mid_x, y: to_y, edge_index });

        (mid_x, (from_y + to_y) / 2.0)
    }
}

fn build_edge_segments(
    graph: &NodeGraphDefinition,
    snap: bool,
) -> (Vec<EdgeSegmentVm>, Vec<EdgeCornerVm>, Vec<EdgeLabelVm>) {
    let mut segments = Vec::new();
    let mut corners = Vec::new();
    let mut labels = Vec::new();
    let thickness = GRID_SIZE * EDGE_THICKNESS_RATIO;
    let mut edge_index: i32 = 0;

    for edge in &graph.edges {
        let from_node = match graph.nodes.iter().find(|n| n.id == edge.from_node_id) {
            Some(node) => node,
            None => continue,
        };
        let to_node = match graph.nodes.iter().find(|n| n.id == edge.to_node_id) {
            Some(node) => node,
            None => continue,
        };

        let (from_x, from_y) = match get_port_center_for_node(from_node, &edge.from_port, false) {
            Some(pos) => pos,
            None => continue,
        };
        let (to_x, to_y) = match get_port_center_for_node(to_node, &edge.to_port, true) {
            Some(pos) => pos,
            None => continue,
        };

        let (from_x, from_y, to_x, to_y) = if snap {
            (
                snap_to_grid_center(from_x),
                snap_to_grid_center(from_y),
                snap_to_grid_center(to_x),
                snap_to_grid_center(to_y),
            )
        } else {
            (from_x, from_y, to_x, to_y)
        };

        let (label_x, label_y) = route_edge(
            from_x, from_y, to_x, to_y,
            thickness, edge_index, snap,
            &mut segments, &mut corners
        );

        let label_text = get_edge_data_type_label(from_node, &edge.from_port)
            .unwrap_or_else(|| "Unknown".to_string());
        let label_width = (label_text.len() as f32 * 7.0).max(GRID_SIZE * 2.0);
        let label_height = GRID_SIZE * 0.8;
       
        labels.push(EdgeLabelVm {
            text: label_text.into(),
            x: label_x,
            y: label_y,
            width: label_width,
            height: label_height,
        });
        
        edge_index += 1;
    }

    (segments, corners, labels)
}

fn build_edges(
    graph: &NodeGraphDefinition,
    selection_state: &crate::ui::selection::SelectionState,
    snap: bool,
) -> Vec<EdgeVm> {
    let selected_edge_from_node = &selection_state.selected_edge_from_node;
    let selected_edge_from_port = &selection_state.selected_edge_from_port;
    let selected_edge_to_node = &selection_state.selected_edge_to_node;
    let selected_edge_to_port = &selection_state.selected_edge_to_port;

    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_node = graph.nodes.iter().find(|n| n.id == edge.from_node_id)?;
            let to_node = graph.nodes.iter().find(|n| n.id == edge.to_node_id)?;

            let (from_x, from_y) = get_port_center_for_node(from_node, &edge.from_port, false)?;
            let (to_x, to_y) = get_port_center_for_node(to_node, &edge.to_port, true)?;

            let (from_x, from_y, to_x, to_y) = if snap {
                (
                    snap_to_grid_center(from_x),
                    snap_to_grid_center(from_y),
                    snap_to_grid_center(to_x),
                    snap_to_grid_center(to_y),
                )
            } else {
                (from_x, from_y, to_x, to_y)
            };

            let is_selected = !selected_edge_from_node.is_empty()
                && edge.from_node_id == selected_edge_from_node.as_str()
                && edge.from_port == selected_edge_from_port.as_str()
                && edge.to_node_id == selected_edge_to_node.as_str()
                && edge.to_port == selected_edge_to_port.as_str();

            Some(EdgeVm {
                from_node_id: edge.from_node_id.clone().into(),
                from_port: edge.from_port.clone().into(),
                to_node_id: edge.to_node_id.clone().into(),
                to_port: edge.to_port.clone().into(),
                from_x: from_x.into(),
                from_y: from_y.into(),
                to_x: to_x.into(),
                to_y: to_y.into(),
                is_selected,
            })
        })
        .collect()
}

fn push_segment(
    segments: &mut Vec<EdgeSegmentVm>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    edge_index: i32,
) {
    if (x1 - x2).abs() < f32::EPSILON && (y1 - y2).abs() < f32::EPSILON {
        return;
    }

    let (x, y, width, height) = if (y1 - y2).abs() < f32::EPSILON {
        let min_x = x1.min(x2);
        let length = (x1 - x2).abs() + thickness;
        (min_x - thickness / 2.0, y1 - thickness / 2.0, length, thickness)
    } else {
        let min_y = y1.min(y2);
        let length = (y1 - y2).abs() + thickness;
        (x1 - thickness / 2.0, min_y - thickness / 2.0, thickness, length)
    };

    segments.push(EdgeSegmentVm {
        x,
        y,
        width,
        height,
        edge_index,
    });
}

fn get_edge_data_type_label(
    node: &crate::node::graph_io::NodeDefinition,
    port_name: &str,
) -> Option<String> {
    node.output_ports
        .iter()
        .find(|p| p.name == port_name)
        .map(|p| p.data_type.to_string())
}

fn build_grid_lines(width: f32, height: f32, grid_size: f32) -> Vec<GridLineVm> {
    let mut lines = Vec::new();
    let mut x = 0.0;
    while x <= width {
        lines.push(GridLineVm {
            x1: x,
            y1: 0.0,
            x2: x,
            y2: height,
        });
        x += grid_size;
    }

    let mut y = 0.0;
    while y <= height {
        lines.push(GridLineVm {
            x1: 0.0,
            y1: y,
            x2: width,
            y2: y,
        });
        y += grid_size;
    }

    lines
}

fn node_dimensions(node: &crate::node::graph_io::NodeDefinition) -> (f32, f32) {
    let min_width = GRID_SIZE * NODE_WIDTH_CELLS;
    let port_rows = node
        .input_ports
        .len()
        .max(node.output_ports.len()) as f32;
    let min_height = GRID_SIZE * (NODE_MIN_ROWS.max(NODE_HEADER_ROWS + port_rows) + NODE_PADDING_BOTTOM);

    match &node.size {
        Some(size) => (size.width.max(min_width), size.height.max(min_height)),
        None => (min_width, min_height),
    }
}
