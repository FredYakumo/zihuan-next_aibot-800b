use slint::{ModelRc, VecModel, SharedString, ComponentHandle};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::error::Result;
use crate::node::graph_io::{
    ensure_positions,
    load_graph_definition_from_json,
    NodeGraphDefinition,
};
use crate::node::registry::NODE_REGISTRY;

use crate::ui::graph_window::{
    EdgeCornerVm, EdgeLabelVm, EdgeSegmentVm, EdgeVm, GridLineVm, NodeGraphWindow, NodeTypeVm,
    NodeVm, PortVm,
};
use crate::ui::selection::{setup_selection_callbacks, BoxSelection};
use crate::ui::window_state::{apply_window_state, load_window_state, save_window_state, WindowState};

const GRID_SIZE: f32 = 20.0;
const NODE_WIDTH_CELLS: f32 = 10.0;
const NODE_HEADER_ROWS: f32 = 2.0;
const NODE_MIN_ROWS: f32 = 3.0;
const NODE_PADDING_BOTTOM: f32 = 0.8;
const CANVAS_WIDTH: f32 = 1200.0;
const CANVAS_HEIGHT: f32 = 800.0;
const EDGE_THICKNESS_RATIO: f32 = 0.3;

#[derive(Debug, Clone)]
enum InlinePortValue {
    Text(String),
    Bool(bool),
}

fn inline_port_key(node_id: &str, port_name: &str) -> String {
    format!("{node_id}::{port_name}")
}

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>) -> Result<()> {
    register_cjk_fonts();

    let ui = NodeGraphWindow::new()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))?;

    if let Some(state) = load_window_state() {
        apply_window_state(&ui.window(), &state);
    }

    let graph_state = Rc::new(RefCell::new(initial_graph.unwrap_or_default()));
    let selection_state = Rc::new(RefCell::new(crate::ui::selection::SelectionState::default()));
    let inline_inputs = Rc::new(RefCell::new(HashMap::<String, InlinePortValue>::new()));

    let current_file = Rc::new(RefCell::new(
        if graph_state.borrow().nodes.is_empty() && graph_state.borrow().edges.is_empty() {
            "未加载 节点图".to_string()
        } else {
            "已加载 节点图".to_string()
        },
    ));

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
    
    let all_node_types = Rc::new(node_types);
    ui.set_grid_size(GRID_SIZE);
    ui.set_edge_thickness(GRID_SIZE * EDGE_THICKNESS_RATIO);

    apply_graph_to_ui(
        &ui,
        &graph_state.borrow(),
        Some(current_file.borrow().clone()),
        &selection_state.borrow(),
        &inline_inputs.borrow(),
    );

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_open_json(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .pick_file()
        {
            if let Ok(graph) = load_graph_definition_from_json(&path) {
                if let Some(ui) = ui_handle.upgrade() {
                    *graph_state_clone.borrow_mut() = graph;
                    let label = path.display().to_string();
                    *current_file_clone.borrow_mut() = label.clone();
                    apply_graph_to_ui(
                        &ui,
                        &graph_state_clone.borrow(),
                        Some(label),
                        &selection_state_clone.borrow(),
                        &inline_inputs_clone.borrow(),
                    );
                }
            }
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_save_json(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .set_file_name("node_graph.json")
            .save_file()
        {
            let graph = graph_state_clone.borrow();
            if let Err(e) = crate::node::graph_io::save_graph_definition_to_json(&path, &graph) {
                eprintln!("Failed to save graph: {}", e);
            } else {
                let label = path.display().to_string();
                *current_file_clone.borrow_mut() = label.clone();
                if let Some(ui) = ui_handle.upgrade() {
                    apply_graph_to_ui(
                        &ui,
                        &graph,
                        Some(label),
                        &selection_state_clone.borrow(),
                        &inline_inputs_clone.borrow(),
                    );
                }
            }
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_add_node(move |type_id: SharedString| {
        let type_id_str = type_id.as_str();
        let mut graph = graph_state_clone.borrow_mut();
        if let Err(e) = add_node_to_graph(&mut graph, type_id_str) {
            eprintln!("Failed to add node: {}", e);
            return;
        }
        let label = "已修改(未保存)".to_string();
        *current_file_clone.borrow_mut() = label.clone();
        if let Some(ui) = ui_handle.upgrade() {
            apply_graph_to_ui(
                &ui,
                &graph,
                Some(label),
                &selection_state_clone.borrow(),
                &inline_inputs_clone.borrow(),
            );
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    ui.on_run_graph(move || {
        let graph_def = graph_state_clone.borrow();
        
        // Build node graph from definition and execute
        match crate::node::registry::build_node_graph_from_definition(&graph_def) {
            Ok(mut node_graph) => {
                println!("开始执行节点图...");
                match node_graph.execute() {
                    Ok(_) => {
                        println!("节点图执行成功!");
                        if let Some(ui) = ui_handle.upgrade() {
                            // Update connection status to show success
                            ui.set_connection_status("✓ 节点图执行成功".into());
                        }
                    }
                    Err(e) => {
                        eprintln!("节点图执行失败: {}", e);
                        if let Some(ui) = ui_handle.upgrade() {
                            ui.invoke_show_error(format!("执行错误：{}", e).into());
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("构建节点图失败: {}", e);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.invoke_show_error(format!("构建节点图失败：{}", e).into());
                }
            }
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Rc::clone(&all_node_types);
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
    let all_node_types_clone = Rc::clone(&all_node_types);
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
    let graph_state_clone = Rc::clone(&graph_state);
    let selection_state_clone = Rc::clone(&selection_state);
    ui.on_node_moved(move |node_id: SharedString, x: f32, y: f32| {
        let mut graph = graph_state_clone.borrow_mut();
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
            if let Some(pos) = &mut node.position {
                pos.x = x;
                pos.y = y;
            } else {
                node.position = Some(crate::node::graph_io::GraphPosition { x, y });
            }
        }
        
        // Update edges based on new node positions during drag (no snapping for smoothness)
        if let Some(ui) = ui_handle.upgrade() {
            let selection = selection_state_clone.borrow();
            let edges = build_edges(&graph, &selection, false);
            let (edge_segments, edge_corners, edge_labels) = build_edge_segments(&graph, false);
            
            ui.set_edges(ModelRc::new(VecModel::from(edges)));
            ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
            ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
            ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let selection_state_clone = Rc::clone(&selection_state);
    ui.on_node_resized(move |node_id: SharedString, width: f32, height: f32| {
        let mut graph = graph_state_clone.borrow_mut();
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
            node.size = Some(crate::node::graph_io::GraphSize { width, height });
        }

        if let Some(ui) = ui_handle.upgrade() {
            let selection = selection_state_clone.borrow();
            let edges = build_edges(&graph, &selection, false);
            let (edge_segments, edge_corners, edge_labels) = build_edge_segments(&graph, false);

            ui.set_edges(ModelRc::new(VecModel::from(edges)));
            ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
            ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
            ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_node_move_finished(move |node_id: SharedString, x: f32, y: f32| {
        let mut graph = graph_state_clone.borrow_mut();
        let snapped_x = snap_to_grid(x);
        let snapped_y = snap_to_grid(y);
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
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

        let label = current_file_clone.borrow().clone();
        if let Some(ui) = ui_handle.upgrade() {
            apply_graph_to_ui(
                &ui,
                &graph,
                Some(label),
                &selection_state_clone.borrow(),
                &inline_inputs_clone.borrow(),
            );
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_node_resize_finished(move |node_id: SharedString, width: f32, height: f32| {
        let mut graph = graph_state_clone.borrow_mut();
        let snapped_width = snap_to_grid(width).max(GRID_SIZE * NODE_WIDTH_CELLS);
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
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

        let label = "已修改(未保存)".to_string();
        *current_file_clone.borrow_mut() = label.clone();
        if let Some(ui) = ui_handle.upgrade() {
            apply_graph_to_ui(
                &ui,
                &graph,
                Some(label),
                &selection_state_clone.borrow(),
                &inline_inputs_clone.borrow(),
            );
        }
    });

    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let port_selection = Rc::new(RefCell::new(None::<(String, String, bool)>));
    let port_selection_for_click = Rc::clone(&port_selection);
    let port_selection_for_move = Rc::clone(&port_selection);
    let port_selection_for_cancel = Rc::clone(&port_selection);
    let ui_handle_for_click = ui.as_weak();
    let ui_handle_for_move = ui.as_weak();
    let ui_handle_for_cancel = ui.as_weak();
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);

    ui.on_port_clicked(move |node_id: SharedString, port_name: SharedString, is_input: bool| {
        let node_id_str = node_id.to_string();
        let port_name_str = port_name.to_string();

        let mut selection = port_selection_for_click.borrow_mut();

        if let Some((prev_node, prev_port, prev_is_input)) = selection.take() {
            if prev_is_input != is_input {
                let mut graph = graph_state_clone.borrow_mut();
                ensure_positions(&mut graph);

                let (from_node, from_port, to_node, to_port) = if is_input {
                    (prev_node, prev_port, node_id_str, port_name_str)
                } else {
                    (node_id_str, port_name_str, prev_node, prev_port)
                };

                graph.edges.push(crate::node::graph_io::EdgeDefinition {
                    from_node_id: from_node,
                    from_port,
                    to_node_id: to_node,
                    to_port,
                });

                let label = "已修改(未保存)".to_string();
                *current_file_clone.borrow_mut() = label.clone();

                if let Some(ui) = ui_handle_for_click.upgrade() {
                    ui.set_drag_line_visible(false);
                    ui.set_show_port_hint(false);
                    ui.set_port_hint_text("".into());
                    apply_graph_to_ui(
                        &ui,
                        &graph,
                        Some(label),
                        &selection_state_clone.borrow(),
                        &inline_inputs_clone.borrow(),
                    );
                }
            } else {
                *selection = Some((prev_node, prev_port, prev_is_input));
            }
        } else {
            *selection = Some((node_id_str.clone(), port_name_str.clone(), is_input));
            if let Some(ui) = ui_handle_for_click.upgrade() {
                let mut graph = graph_state_clone.borrow_mut();
                ensure_positions(&mut graph);
                if let Some((from_x, from_y)) =
                    get_port_center(&graph, node_id_str.as_str(), port_name_str.as_str(), is_input)
                {
                    ui.set_drag_line_visible(true);
                    ui.set_drag_line_from_x(from_x);
                    ui.set_drag_line_from_y(from_y);
                    ui.set_drag_line_to_x(from_x);
                    ui.set_drag_line_to_y(from_y);
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
        if port_selection_for_move.borrow().is_none() {
            return;
        }

        if let Some(ui) = ui_handle_for_move.upgrade() {
            ui.set_drag_line_to_x(snap_to_grid_center(x));
            ui.set_drag_line_to_y(snap_to_grid_center(y));
        }
    });

    ui.on_cancel_connect(move || {
        *port_selection_for_cancel.borrow_mut() = None;
        if let Some(ui) = ui_handle_for_cancel.upgrade() {
            ui.set_drag_line_visible(false);
            ui.set_show_port_hint(false);
            ui.set_port_hint_text("".into());
        }
    });

    // Setup selection callbacks using the selection module
    let selection_state_for_cb = Rc::clone(&selection_state);
    let inline_inputs_for_cb = Rc::clone(&inline_inputs);
    let apply_graph_fn = move |ui: &NodeGraphWindow, graph: &NodeGraphDefinition, file: Option<String>| {
        apply_graph_to_ui(
            ui,
            graph,
            file,
            &selection_state_for_cb.borrow(),
            &inline_inputs_for_cb.borrow(),
        );
    };
    
    setup_selection_callbacks(
        &ui, 
        Rc::clone(&graph_state), 
        Rc::clone(&current_file), 
        apply_graph_fn,
        Rc::clone(&selection_state)
    );
    
    // Setup box selection
    let box_selection = Rc::new(RefCell::new(BoxSelection::new()));
    
    let ui_handle = ui.as_weak();
    let box_selection_clone = Rc::clone(&box_selection);
    ui.on_box_selection_start(move |x: f32, y: f32| {
        let mut box_sel = box_selection_clone.borrow_mut();
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
    let box_selection_clone = Rc::clone(&box_selection);
    ui.on_box_selection_update(move |x: f32, y: f32| {
        let mut box_sel = box_selection_clone.borrow_mut();
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
    let box_selection_clone = Rc::clone(&box_selection);
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let selection_state_clone = Rc::clone(&selection_state);
    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_box_selection_end(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut box_sel = box_selection_clone.borrow_mut();
            let graph = graph_state_clone.borrow();
            
            // Find nodes within the selection box
            let mut selected_nodes = Vec::new();
            for node in &graph.nodes {
                if let Some(pos) = &node.position {
                    let (node_width, node_height) = node_dimensions(node);
                    
                    if box_sel.contains_rect(pos.x, pos.y, node_width, node_height) {
                        selected_nodes.push(node.id.clone());
                    }
                }
            }
            
            let mut selection = selection_state_clone.borrow_mut();
            // Clear existing selection and select all found nodes
            selection.clear();
            for node_id in selected_nodes {
                selection.select_node(node_id, true);
            }
            selection.apply_to_ui(&ui); // To update count and other properties
            
            let label = current_file_clone.borrow().clone();
            apply_graph_to_ui(
                &ui,
                &graph,
                Some(label),
                &selection.clone(),
                &inline_inputs_clone.borrow(),
            );
            
            box_sel.end();
            ui.set_box_selection_visible(false);
        }
    });

    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_inline_port_text_changed(move |node_id: SharedString, port_name: SharedString, value: SharedString| {
        let key = inline_port_key(node_id.as_str(), port_name.as_str());
        inline_inputs_clone
            .borrow_mut()
            .insert(key, InlinePortValue::Text(value.to_string()));
    });

    let inline_inputs_clone = Rc::clone(&inline_inputs);
    ui.on_inline_port_bool_changed(move |node_id: SharedString, port_name: SharedString, value: bool| {
        let key = inline_port_key(node_id.as_str(), port_name.as_str());
        inline_inputs_clone
            .borrow_mut()
            .insert(key, InlinePortValue::Bool(value));
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
            
            let input_ports: Vec<PortVm> = node
                .input_ports
                .iter()
                .map(|p| {
                    let is_connected = graph.edges.iter().any(|e| {
                        e.to_node_id == node.id && e.to_port == p.name
                    });
                    let key = inline_port_key(&node.id, &p.name);
                    let (inline_text, inline_bool) = match &p.data_type {
                        crate::node::DataType::Boolean => {
                            let value = match inline_inputs.get(&key) {
                                Some(InlinePortValue::Bool(v)) => *v,
                                Some(InlinePortValue::Text(v)) => v.eq_ignore_ascii_case("true"),
                                None => false,
                            };
                            (String::new(), value)
                        }
                        crate::node::DataType::String
                        | crate::node::DataType::Integer
                        | crate::node::DataType::Float => {
                            let value = match inline_inputs.get(&key) {
                                Some(InlinePortValue::Text(v)) => v.clone(),
                                Some(InlinePortValue::Bool(v)) => v.to_string(),
                                None => String::new(),
                            };
                            (value, false)
                        }
                        _ => (String::new(), false),
                    };
                    PortVm {
                        name: p.name.clone().into(),
                        is_input: true,
                        is_connected,
                        is_required: p.required,
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
                        data_type: p.data_type.to_string().into(),
                        inline_text: "".into(),
                        inline_bool: false,
                    }
                })
                .collect();

            NodeVm {
                id: node.id.clone().into(),
                label: label.into(),
                x: position.map(|p| snap_to_grid(p.x)).unwrap_or(0.0),
                y: position.map(|p| snap_to_grid(p.y)).unwrap_or(0.0),
                width: node_width,
                height: node_height,
                input_ports: ModelRc::new(VecModel::from(input_ports)),
                output_ports: ModelRc::new(VecModel::from(output_ports)),
                is_selected,
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

fn build_edge_segments(
    graph: &NodeGraphDefinition,
    snap: bool,
) -> (Vec<EdgeSegmentVm>, Vec<EdgeCornerVm>, Vec<EdgeLabelVm>) {
    let mut segments = Vec::new();
    let mut corners = Vec::new();
    let mut labels = Vec::new();
    let thickness = GRID_SIZE * EDGE_THICKNESS_RATIO;

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

        let mid_x = if snap {
            snap_to_grid_center((from_x + to_x) / 2.0)
        } else {
            (from_x + to_x) / 2.0
        };

        push_segment(&mut segments, from_x, from_y, mid_x, from_y, thickness);
        push_segment(&mut segments, mid_x, from_y, mid_x, to_y, thickness);
        push_segment(&mut segments, mid_x, to_y, to_x, to_y, thickness);
        corners.push(EdgeCornerVm { x: mid_x, y: from_y });
        corners.push(EdgeCornerVm { x: mid_x, y: to_y });

        let label_text = get_edge_data_type_label(from_node, &edge.from_port)
            .unwrap_or_else(|| "Unknown".to_string());
        let label_width = (label_text.len() as f32 * 7.0).max(GRID_SIZE * 2.0);
        let label_height = GRID_SIZE * 0.8;
        labels.push(EdgeLabelVm {
            text: label_text.into(),
            x: mid_x,
            y: (from_y + to_y) / 2.0,
            width: label_width,
            height: label_height,
        });
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
