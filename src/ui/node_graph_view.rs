use slint::{ModelRc, VecModel, SharedString, ComponentHandle};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::error::Result;
use crate::node::graph_io::{
    ensure_positions,
    load_graph_definition_from_json,
    NodeGraphDefinition,
};
use crate::node::registry::NODE_REGISTRY;

use crate::ui::graph_window::{NodeGraphWindow, NodeTypeVm, NodeVm, EdgeVm, PortVm};

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>) -> Result<()> {
    register_cjk_fonts();

    let ui = NodeGraphWindow::new()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))?;

    let graph_state = Rc::new(RefCell::new(initial_graph.unwrap_or_default()));
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
    
    ui.set_available_node_types(ModelRc::new(VecModel::from(node_types)));

    apply_graph_to_ui(
        &ui,
        &graph_state.borrow(),
        Some(current_file.borrow().clone()),
    );

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
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
                    apply_graph_to_ui(&ui, &graph_state_clone.borrow(), Some(label));
                }
            }
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
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
            apply_graph_to_ui(&ui, &graph, Some(label));
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_show_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
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
    let graph_state_clone = Rc::clone(&graph_state);
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
        
        // Update edges based on new node positions during drag
        if let Some(ui) = ui_handle.upgrade() {
            let edges: Vec<EdgeVm> = graph
                .edges
                .iter()
                .filter_map(|edge| {
                    let from_node = graph.nodes.iter().find(|n| n.id == edge.from_node_id)?;
                    let to_node = graph.nodes.iter().find(|n| n.id == edge.to_node_id)?;
                    
                    let from_pos = from_node.position.as_ref()?;
                    let to_pos = to_node.position.as_ref()?;
                    
                    let from_x = from_pos.x + 180.0;
                    let from_y = from_pos.y + 50.0;
                    
                    let to_x = to_pos.x;
                    let to_y = to_pos.y + 50.0;
                    
                    Some(EdgeVm {
                        from_node_id: edge.from_node_id.clone().into(),
                        from_port: edge.from_port.clone().into(),
                        to_node_id: edge.to_node_id.clone().into(),
                        to_port: edge.to_port.clone().into(),
                        from_x: from_x.into(),
                        from_y: from_y.into(),
                        to_x: to_x.into(),
                        to_y: to_y.into(),
                    })
                })
                .collect();
            
            ui.set_edges(ModelRc::new(VecModel::from(edges)));
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    ui.on_node_move_finished(move |node_id: SharedString, x: f32, y: f32| {
        let mut graph = graph_state_clone.borrow_mut();
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
            if let Some(pos) = &mut node.position {
                pos.x = x;
                pos.y = y;
            } else {
                node.position = Some(crate::node::graph_io::GraphPosition { x, y });
            }
        }

        let label = current_file_clone.borrow().clone();
        if let Some(ui) = ui_handle.upgrade() {
            apply_graph_to_ui(&ui, &graph, Some(label));
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let port_selection = Rc::new(RefCell::new(None::<(String, String, bool)>));
    ui.on_port_clicked(move |node_id: SharedString, port_name: SharedString, is_input: bool| {
        let node_id_str = node_id.to_string();
        let port_name_str = port_name.to_string();
        
        let mut selection = port_selection.borrow_mut();
        
        if let Some((prev_node, prev_port, prev_is_input)) = selection.take() {
            // Second click - create edge if valid
            if prev_is_input != is_input {
                let mut graph = graph_state_clone.borrow_mut();
                
                // Determine direction: output -> input
                let (from_node, from_port, to_node, to_port) = if is_input {
                    (prev_node, prev_port, node_id_str, port_name_str)
                } else {
                    (node_id_str, port_name_str, prev_node, prev_port)
                };
                
                // Add edge
                graph.edges.push(crate::node::graph_io::EdgeDefinition {
                    from_node_id: from_node,
                    from_port,
                    to_node_id: to_node,
                    to_port,
                });
                
                // Update UI
                if let Some(ui) = ui_handle.upgrade() {
                    apply_graph_to_ui(&ui, &graph, Some("已修改(未保存)".to_string()));
                }
            }
        } else {
            // First click - store selection
            *selection = Some((node_id_str, port_name_str, is_input));
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    ui.on_drag_end_at(move |from_node: SharedString,
                            from_port: SharedString,
                            from_is_input: bool,
                            x: f32,
                            y: f32| {
        let from_node_str = from_node.to_string();
        let from_port_str = from_port.to_string();

        let mut graph = graph_state_clone.borrow_mut();

        let target = find_port_at(&graph, x, y);
        let Some((to_node_str, to_port_str, to_is_input)) = target else {
            return;
        };

        if from_is_input == to_is_input {
            return;
        }

        if from_node_str == to_node_str && from_port_str == to_port_str {
            return;
        }

        let (edge_from_node, edge_from_port, edge_to_node, edge_to_port) = if from_is_input {
            (to_node_str, to_port_str, from_node_str, from_port_str)
        } else {
            (from_node_str, from_port_str, to_node_str, to_port_str)
        };

        graph.edges.push(crate::node::graph_io::EdgeDefinition {
            from_node_id: edge_from_node,
            from_port: edge_from_port,
            to_node_id: edge_to_node,
            to_port: edge_to_port,
        });

        let label = "已修改(未保存)".to_string();
        *current_file_clone.borrow_mut() = label.clone();

        if let Some(ui) = ui_handle.upgrade() {
            apply_graph_to_ui(&ui, &graph, Some(label));
        }
    });

    ui.run()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))
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
) {
    let mut graph = graph.clone();
    ensure_positions(&mut graph);

    let nodes: Vec<NodeVm> = graph
        .nodes
        .iter()
        .map(|node| {
            let position = node.position.as_ref();
            let label = format!("{}", node.name);
            
            let input_ports: Vec<PortVm> = node
                .input_ports
                .iter()
                .map(|p| PortVm {
                    name: p.name.clone().into(),
                    is_input: true,
                })
                .collect();
            
            let output_ports: Vec<PortVm> = node
                .output_ports
                .iter()
                .map(|p| PortVm {
                    name: p.name.clone().into(),
                    is_input: false,
                })
                .collect();

            NodeVm {
                id: node.id.clone().into(),
                label: label.into(),
                x: position.map(|p| p.x).unwrap_or(0.0),
                y: position.map(|p| p.y).unwrap_or(0.0),
                input_ports: ModelRc::new(VecModel::from(input_ports)),
                output_ports: ModelRc::new(VecModel::from(output_ports)),
            }
        })
        .collect();

    // Calculate edge visual positions based on node positions
    let edges: Vec<EdgeVm> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_node = graph.nodes.iter().find(|n| n.id == edge.from_node_id)?;
            let to_node = graph.nodes.iter().find(|n| n.id == edge.to_node_id)?;
            
            let from_pos = from_node.position.as_ref()?;
            let to_pos = to_node.position.as_ref()?;
            
            // Calculate port offset (right side for output, left side for input)
            let from_x = from_pos.x + 180.0; // Right edge of node
            let from_y = from_pos.y + 50.0; // Approximate port Y position
            
            let to_x = to_pos.x; // Left edge of node
            let to_y = to_pos.y + 50.0; // Approximate port Y position
            
            Some(EdgeVm {
                from_node_id: edge.from_node_id.clone().into(),
                from_port: edge.from_port.clone().into(),
                to_node_id: edge.to_node_id.clone().into(),
                to_port: edge.to_port.clone().into(),
                from_x: from_x.into(),
                from_y: from_y.into(),
                to_x: to_x.into(),
                to_y: to_y.into(),
            })
        })
        .collect();

    let label = current_file.unwrap_or_else(|| "已加载 JSON".to_string());

    ui.set_nodes(ModelRc::new(VecModel::from(nodes)));
    ui.set_edges(ModelRc::new(VecModel::from(edges)));
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
    const NODE_WIDTH: f32 = 180.0;
    const PADDING: f32 = 10.0;
    const TITLE_HEIGHT: f32 = 22.0;
    const TITLE_SPACING: f32 = 6.0;
    const ROW_HEIGHT: f32 = 20.0;
    const ROW_SPACING: f32 = 4.0;
    const PORT_RADIUS: f32 = 8.0;

    let input_x = PADDING + 5.0;
    let output_x = NODE_WIDTH - PADDING - 5.0;
    let base_y_offset = PADDING + TITLE_HEIGHT + TITLE_SPACING;

    let radius_sq = PORT_RADIUS * PORT_RADIUS;

    for node in &graph.nodes {
        let position = match node.position.as_ref() {
            Some(pos) => pos,
            None => continue,
        };

        let node_x = position.x;
        let node_y = position.y;

        for (index, port) in node.input_ports.iter().enumerate() {
            let center_x = node_x + input_x;
            let center_y = node_y
                + base_y_offset
                + index as f32 * (ROW_HEIGHT + ROW_SPACING)
                + ROW_HEIGHT / 2.0;

            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_sq {
                return Some((node.id.clone(), port.name.clone(), true));
            }
        }

        for (index, port) in node.output_ports.iter().enumerate() {
            let center_x = node_x + output_x;
            let center_y = node_y
                + base_y_offset
                + index as f32 * (ROW_HEIGHT + ROW_SPACING)
                + ROW_HEIGHT / 2.0;

            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_sq {
                return Some((node.id.clone(), port.name.clone(), false));
            }
        }
    }

    None
}
