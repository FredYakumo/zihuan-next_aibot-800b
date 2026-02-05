use slint::{ModelRc, VecModel};
use std::path::Path;

use crate::error::Result;
use crate::node::graph_io::{
    ensure_positions,
    load_graph_definition_from_json,
    NodeGraphDefinition,
};

slint::slint! {
    import { HorizontalBox, VerticalBox } from "std-widgets.slint";

    export struct NodeVm {
        label: string,
        x: float,
        y: float,
    }

    export struct EdgeVm {
        label: string,
    }

    component CjkText inherits Text {
    }

    component CjkButton inherits Rectangle {
        in property <string> text;
        callback clicked();

        width: 120px;
        height: 32px;
        background: #2f2f2f;
        border-radius: 6px;
        border-width: 1px;
        border-color: #4a4a4a;

        TouchArea {
            clicked => { root.clicked(); }
        }

        CjkText {
            text: root.text;
            color: #f0f0f0;
            vertical-alignment: center;
            horizontal-alignment: center;
            font-size: 12px;
        }
    }

    component NodeItem inherits Rectangle {
        in property <string> label;
        in property <float> x_pos;
        in property <float> y_pos;

        x: x_pos * 1px;
        y: y_pos * 1px;
        width: 160px;
        height: 56px;
        background: #2b2b2b;
        border-radius: 8px;
        border-width: 1px;
        border-color: #4a4a4a;

        CjkText {
            text: label;
            color: #f0f0f0;
            vertical-alignment: center;
            horizontal-alignment: center;
            font-size: 14px;
        }
    }

    component GraphCanvas inherits Rectangle {
        in property <[NodeVm]> nodes;
        background: #1e1e1e;

        for node in nodes: NodeItem {
            label: node.label;
            x_pos: node.x;
            y_pos: node.y;
        }
    }

    export component NodeGraphWindow inherits Window {
        in property <[NodeVm]> nodes;
        in property <[EdgeVm]> edges;
        in property <string> current_file;

        callback open_json();

        title: "Zihuan Node Graph Viewer";
        width: 1200px;
        height: 800px;

        HorizontalBox {
            spacing: 12px;
            padding: 12px;

            GraphCanvas {
                width: 860px;
                height: 760px;
                nodes: root.nodes;
            }

            VerticalBox {
                width: 300px;
                height: 760px;
                spacing: 8px;

                CjkButton {
                    text: "读取节点图文件";
                    clicked => { root.open_json(); }
                }

                CjkText {
                    text: root.current_file;
                    font-size: 12px;
                    color: #555555;
                    wrap: word-wrap;
                }

                CjkText {
                    text: "Edges";
                    font-size: 16px;
                    color: #222222;
                }

                for edge in edges: CjkText {
                    text: edge.label;
                    font-size: 12px;
                    color: #444444;
                }
            }
        }
    }
}

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>) -> Result<()> {
    register_cjk_fonts();

    let ui = NodeGraphWindow::new()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))?;

    if let Some(graph) = initial_graph {
        apply_graph_to_ui(&ui, &graph, Some("已加载 节点图".to_string()));
    } else {
        ui.set_current_file("未加载 节点图".into());
    }

    let ui_handle = ui.as_weak();
    ui.on_open_json(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .pick_file()
        {
            if let Ok(graph) = load_graph_definition_from_json(&path) {
                if let Some(ui) = ui_handle.upgrade() {
                    apply_graph_to_ui(&ui, &graph, Some(path.display().to_string()));
                }
            }
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
            let label = format!("{} ({})", node.name, node.id);
            NodeVm {
                label: label.into(),
                x: position.map(|p| p.x).unwrap_or(0.0),
                y: position.map(|p| p.y).unwrap_or(0.0),
            }
        })
        .collect();

    let edges: Vec<EdgeVm> = graph
        .edges
        .iter()
        .map(|edge| EdgeVm {
            label: format!(
                "{}:{} → {}:{}",
                edge.from_node_id, edge.from_port, edge.to_node_id, edge.to_port
            )
            .into(),
        })
        .collect();

    let label = current_file.unwrap_or_else(|| "已加载 JSON".to_string());

    ui.set_nodes(ModelRc::new(VecModel::from(nodes)));
    ui.set_edges(ModelRc::new(VecModel::from(edges)));
    ui.set_current_file(label.into());
}
