// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;

use petgraph::data::Build;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use petgraph::{Graph};
use petgraph::algo::astar;

#[derive(Deserialize, Serialize)]
struct JsNode {
    id: String,
}

#[derive(Deserialize, Serialize)]
struct JsEdge {
    source: String,
    target: String,
    weight: u16,
}

#[derive(Deserialize, Serialize)]
struct JsGraph {
    nodes: Vec<JsNode>,
    edges: Vec<JsEdge>,
}

#[derive(Serialize)]
struct ShortestPath {
    length: u16,
    path: Vec<String>,
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn shortest_path(js_graph: JsGraph, source: &str, target: &str) -> Option<ShortestPath> {
    let mut graph = Graph::new();
    let mut map = HashMap::new();
    for node in js_graph.nodes {
        let node_id = graph.add_node(node.id.clone());
        map.insert(node.id, node_id);
    }
    for edge in js_graph.edges {
        graph.add_edge(*map.get(&edge.source).unwrap(), *map.get(&edge.target).unwrap(), edge.weight);
    }
    
    let end = *map.get(target).unwrap();
    
    let (length, path) = astar(&graph,  *map.get(source).unwrap(), |id| id == end, |edge| *edge.weight(), |_| 0)?;
    
    let mut result = Vec::with_capacity(path.len());
    for node in path {
        result.push(graph[node].clone()); 
    }
    
    Some(ShortestPath {
        length,
        path: result
    })
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![shortest_path])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
