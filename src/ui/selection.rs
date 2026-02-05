use std::cell::RefCell;
use std::rc::Rc;
use std::collections::HashSet;
use slint::{ComponentHandle, SharedString};

use crate::node::graph_io::NodeGraphDefinition;
use crate::ui::graph_window::NodeGraphWindow;

/// Selection state manager for nodes and edges
#[derive(Default, Clone)]
pub struct SelectionState {
    pub selected_node_ids: HashSet<String>,
    pub selected_edge_from_node: String,
    pub selected_edge_from_port: String,
    pub selected_edge_to_node: String,
    pub selected_edge_to_port: String,
}

impl SelectionState {
    pub fn clear(&mut self) {
        self.selected_node_ids.clear();
        self.selected_edge_from_node.clear();
        self.selected_edge_from_port.clear();
        self.selected_edge_to_node.clear();
        self.selected_edge_to_port.clear();
    }

    pub fn select_node(&mut self, node_id: String, multi_select: bool) {
        if !multi_select {
            self.selected_node_ids.clear();
            self.selected_edge_from_node.clear();
            self.selected_edge_from_port.clear();
            self.selected_edge_to_node.clear();
            self.selected_edge_to_port.clear();
        }
        self.selected_node_ids.insert(node_id);
    }
    
    pub fn toggle_node_selection(&mut self, node_id: String) {
        if self.selected_node_ids.contains(&node_id) {
            self.selected_node_ids.remove(&node_id);
        } else {
            self.selected_node_ids.insert(node_id);
        }
        // Clear edge selection when modifying node selection
        self.selected_edge_from_node.clear();
        self.selected_edge_from_port.clear();
        self.selected_edge_to_node.clear();
        self.selected_edge_to_port.clear();
    }

    pub fn select_edge(&mut self, from_node: String, from_port: String, to_node: String, to_port: String) {
        self.selected_node_ids.clear();
        self.selected_edge_from_node = from_node;
        self.selected_edge_from_port = from_port;
        self.selected_edge_to_node = to_node;
        self.selected_edge_to_port = to_port;
    }

    pub fn has_selection(&self) -> bool {
        !self.selected_node_ids.is_empty() || !self.selected_edge_from_node.is_empty()
    }

    pub fn apply_to_ui(&self, ui: &NodeGraphWindow) {
        ui.set_selected_node_count(self.selected_node_ids.len() as i32);
        ui.set_selected_edge_from_node(self.selected_edge_from_node.clone().into());
        ui.set_selected_edge_from_port(self.selected_edge_from_port.clone().into());
        ui.set_selected_edge_to_node(self.selected_edge_to_node.clone().into());
        ui.set_selected_edge_to_port(self.selected_edge_to_port.clone().into());
    }
}

/// Setup selection-related callbacks for the UI
pub fn setup_selection_callbacks(
    ui: &NodeGraphWindow,
    graph_state: Rc<RefCell<NodeGraphDefinition>>,
    current_file: Rc<RefCell<String>>,
    apply_graph_fn: impl Fn(&NodeGraphWindow, &NodeGraphDefinition, Option<String>) + 'static,
    selection_state: Rc<RefCell<SelectionState>>,
) {
    // Handle node click for selection
    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let apply_graph_fn_1 = Rc::new(apply_graph_fn);
    let apply_graph_fn_clone = Rc::clone(&apply_graph_fn_1);
    let selection_state_clone = Rc::clone(&selection_state);
    
    ui.on_node_clicked(move |node_id: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            {
                let mut selection = selection_state_clone.borrow_mut();
                // TODO: Detect modifier keys for multi-select toggle
                selection.select_node(node_id.to_string(), false);
                selection.apply_to_ui(&ui);
            }
            
            let graph = graph_state_clone.borrow();
            let label = current_file_clone.borrow().clone();
            apply_graph_fn_clone(&ui, &graph, Some(label));
        }
    });

    // Handle edge click for selection
    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let apply_graph_fn_clone = Rc::clone(&apply_graph_fn_1);
    let selection_state_clone = Rc::clone(&selection_state);

    ui.on_edge_clicked(move |from_node: SharedString, from_port: SharedString, to_node: SharedString, to_port: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            {
                let mut selection = selection_state_clone.borrow_mut();
                selection.select_edge(
                    from_node.to_string(),
                    from_port.to_string(),
                    to_node.to_string(),
                    to_port.to_string(),
                );
                selection.apply_to_ui(&ui);
            }
            
            let graph = graph_state_clone.borrow();
            let label = current_file_clone.borrow().clone();
            apply_graph_fn_clone(&ui, &graph, Some(label));
        }
    });

    // Handle canvas click to clear selection
    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let apply_graph_fn_clone = Rc::clone(&apply_graph_fn_1);
    let selection_state_clone = Rc::clone(&selection_state);

    ui.on_canvas_clicked(move || {
        if let Some(ui) = ui_handle.upgrade() {
            {
                let mut selection = selection_state_clone.borrow_mut();
                selection.clear();
                selection.apply_to_ui(&ui);
            }
            
            let graph = graph_state_clone.borrow();
            let label = current_file_clone.borrow().clone();
            apply_graph_fn_clone(&ui, &graph, Some(label));
        }
    });

    // Handle delete selected element
    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    let apply_graph_fn_clone = Rc::clone(&apply_graph_fn_1);
    let selection_state_clone = Rc::clone(&selection_state);

    ui.on_delete_selected(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut graph = graph_state_clone.borrow_mut();
            {
                let mut selection = selection_state_clone.borrow_mut();
                
                if !selection.selected_node_ids.is_empty() {
                    // Delete nodes and all connected edges
                    graph.nodes.retain(|n| !selection.selected_node_ids.contains(&n.id));
                    graph.edges.retain(|e| {
                        !selection.selected_node_ids.contains(&e.from_node_id) 
                        && !selection.selected_node_ids.contains(&e.to_node_id)
                    });
                } else if !selection.selected_edge_from_node.is_empty() {
                    // Delete edge
                    graph.edges.retain(|e| {
                        !(e.from_node_id == selection.selected_edge_from_node
                        && e.from_port == selection.selected_edge_from_port
                        && e.to_node_id == selection.selected_edge_to_node
                        && e.to_port == selection.selected_edge_to_port)
                    });
                }
                
                selection.clear();
                selection.apply_to_ui(&ui);
            }
            
            let label = "已修改(未保存)".to_string();
            *current_file_clone.borrow_mut() = label.clone();
            apply_graph_fn_clone(&ui, &graph, Some(label));
        }
    });
}

/// Box selection helper
pub struct BoxSelection {
    pub start_x: f32,
    pub start_y: f32,
    pub end_x: f32,
    pub end_y: f32,
    pub active: bool,
}

impl BoxSelection {
    pub fn new() -> Self {
        Self {
            start_x: 0.0,
            start_y: 0.0,
            end_x: 0.0,
            end_y: 0.0,
            active: false,
        }
    }

    pub fn start(&mut self, x: f32, y: f32) {
        self.start_x = x;
        self.start_y = y;
        self.end_x = x;
        self.end_y = y;
        self.active = true;
    }

    pub fn update(&mut self, x: f32, y: f32) {
        self.end_x = x;
        self.end_y = y;
    }

    pub fn end(&mut self) {
        self.active = false;
    }

    pub fn get_bounds(&self) -> (f32, f32, f32, f32) {
        let min_x = self.start_x.min(self.end_x);
        let max_x = self.start_x.max(self.end_x);
        let min_y = self.start_y.min(self.end_y);
        let max_y = self.start_y.max(self.end_y);
        (min_x, min_y, max_x, max_y)
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let (min_x, min_y, max_x, max_y) = self.get_bounds();
        x >= min_x && x <= max_x && y >= min_y && y <= max_y
    }

    pub fn contains_rect(&self, rect_x: f32, rect_y: f32, rect_width: f32, rect_height: f32) -> bool {
        let (min_x, min_y, max_x, max_y) = self.get_bounds();
        
        // Check if any corner of the rectangle is inside the selection box
        self.contains_point(rect_x, rect_y)
            || self.contains_point(rect_x + rect_width, rect_y)
            || self.contains_point(rect_x, rect_y + rect_height)
            || self.contains_point(rect_x + rect_width, rect_y + rect_height)
            // Or check if the selection box is completely inside the rectangle
            || (rect_x <= min_x && rect_x + rect_width >= max_x 
                && rect_y <= min_y && rect_y + rect_height >= max_y)
    }
}

impl Default for BoxSelection {
    fn default() -> Self {
        Self::new()
    }
}
